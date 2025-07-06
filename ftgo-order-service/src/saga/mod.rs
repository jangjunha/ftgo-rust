use std::collections::HashMap;

use crate::{
    models::{self, SagaInstance},
    schema,
};
use diesel::{insert_into, prelude::*, update, PgConnection};
use ftgo_proto::common::CommandReply;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use uuid::Uuid;

const SAGA_HEADER_TYPE: &'static str = "SAGA-TYPE";
const SAGA_HEADER_ID: &'static str = "SAGA-ID";

pub struct SagaStep<'a, Data> {
    pub invoke: Option<
        Box<
            dyn Fn(
                    &Data,
                    &HashMap<String, String>,
                    &mut PgConnection,
                ) -> Result<String, diesel::result::Error>
                + 'a,
        >,
    >,
    pub on_reply: Option<Box<dyn Fn(Data, &CommandReply) -> Data + 'a>>,
    pub invoke_compensation: Option<
        Box<
            dyn Fn(
                    &Data,
                    &HashMap<String, String>,
                    &mut PgConnection,
                ) -> Result<String, diesel::result::Error>
                + 'a,
        >,
    >,
}

pub struct SagaDefition<'a, Data> {
    pub steps: Vec<SagaStep<'a, Data>>,
}

pub trait Saga<Data: Serialize + DeserializeOwned> {
    fn r#type(&self) -> &'static str;
    fn get_definition(&self) -> &SagaDefition<Data>;
    fn serialize_data(&self, data: &Data) -> Value {
        serde_json::to_value(data).unwrap()
    }
    fn deserialize_data(&self, data: &Value) -> Data {
        serde_json::from_value(data.clone()).unwrap()
    }
}

pub struct SagaManager<'a, Data: Serialize> {
    pub saga: Box<dyn Saga<Data>>,
    connection: &'a mut PgConnection,
}

impl<'a, Data: Serialize + DeserializeOwned> SagaManager<'a, Data> {
    pub fn new(saga: impl Saga<Data> + 'static, connection: &'a mut PgConnection) -> Self {
        Self {
            saga: Box::new(saga),
            connection,
        }
    }

    pub fn create(&mut self, saga_data: Data) -> Result<SagaInstance, diesel::result::Error> {
        let saga_instance = SagaInstance {
            saga_type: self.saga.r#type().to_string(),
            saga_id: Uuid::new_v4().to_string(),
            currently_executing: -1,
            last_request_id: None,
            end_state: false,
            compensating: false,
            failed: false,
            saga_data_json: serde_json::to_value(&saga_data).unwrap(),
        };
        insert_into(schema::saga_instances::table)
            .values(&saga_instance)
            .execute(self.connection)?;

        self.process(saga_instance, &saga_data, true)
    }

    pub fn handle_reply(&mut self, message: &CommandReply) -> Result<(), diesel::result::Error> {
        let saga_type = match message.state.get(SAGA_HEADER_TYPE) {
            Some(s) if s == self.saga.r#type() => s,
            _ => {
                return Ok(());
            }
        };
        println!("Handle saga reply: {:?}", message);

        let saga_id = message.state.get(SAGA_HEADER_ID).expect(&format!(
            "`{}` should exists on state of saga reply message",
            SAGA_HEADER_ID
        ));
        let mut saga_instance = schema::saga_instances::table
            .select(models::SagaInstance::as_select())
            .find((saga_type, saga_id))
            .get_result::<models::SagaInstance>(self.connection)?;
        let saga_data = self.saga.deserialize_data(&saga_instance.saga_data_json);

        let current_step = {
            let idx: usize = saga_instance.currently_executing.try_into().unwrap();
            &self.saga.get_definition().steps[idx]
        };
        let saga_data = match (saga_instance.compensating, &current_step.on_reply) {
            (false, Some(on_reply)) => on_reply(saga_data, message),
            (false, None) => saga_data,
            (true, _) => saga_data,
        };
        saga_instance.saga_data_json = serde_json::to_value(&saga_data).unwrap();

        println!(
            "Current executing={}, compensating={}, end_state={}, failed={}",
            saga_instance.currently_executing,
            saga_instance.compensating,
            saga_instance.end_state,
            saga_instance.failed,
        );

        self.process(saga_instance, &saga_data, message.succeed)?;

        Ok(())
    }

    fn process(
        &mut self,
        mut saga_instance: SagaInstance,
        saga_data: &Data,
        succeed: bool,
    ) -> Result<SagaInstance, diesel::result::Error> {
        match (saga_instance.compensating, succeed) {
            (_, true) => {
                // Resume
                saga_instance = match self.next(saga_instance, saga_data) {
                    (saga_instance, Ok(())) => saga_instance,
                    (saga_instance, Err(err)) => {
                        println!(
                            "Failed to process saga (instance={:?}, succeed={}): {:?}",
                            saga_instance, succeed, err
                        );
                        return self.process(saga_instance, saga_data, false);
                    }
                }
            }
            (false, false) => {
                // Start compensating
                saga_instance.compensating = true;
                saga_instance = match self.next(saga_instance, saga_data) {
                    (saga_instance, Ok(())) => saga_instance,
                    (saga_instance, Err(err)) => {
                        println!(
                            "Failed to process saga (instance={:?}, succeed={}): {:?}",
                            saga_instance, succeed, err
                        );
                        return self.process(saga_instance, saga_data, false);
                    }
                }
            }
            (true, false) => {
                // Fail to compensate
                saga_instance.end_state = true;
                saga_instance.failed = true;
            }
        };
        update(schema::saga_instances::table)
            .set(&saga_instance)
            .filter(
                schema::saga_instances::saga_type
                    .eq(&saga_instance.saga_type)
                    .and(schema::saga_instances::saga_id.eq(&saga_instance.saga_id)),
            )
            .execute(self.connection)?;
        Ok(saga_instance)
    }

    /// Should only called by process()
    fn next(
        &mut self,
        mut saga_instance: SagaInstance,
        saga_data: &Data,
    ) -> (SagaInstance, Result<(), diesel::result::Error>) {
        let steps = &self.saga.get_definition().steps;
        let direction = if saga_instance.compensating { -1 } else { 1 };
        let mut i = saga_instance.currently_executing + direction;
        while i >= 0 && i < steps.len().try_into().unwrap() {
            let idx: usize = i.try_into().unwrap();
            let step = &steps[idx];
            saga_instance.currently_executing = i;
            println!(
                "Next executing={}, compensating={}",
                saga_instance.currently_executing, saga_instance.compensating,
            );

            let func = if saga_instance.compensating {
                &step.invoke_compensation
            } else {
                &step.invoke
            };

            if let Some(func) = func {
                let saga_headers = HashMap::from([
                    (
                        SAGA_HEADER_TYPE.to_string(),
                        saga_instance.saga_type.to_string(),
                    ),
                    (
                        SAGA_HEADER_ID.to_string(),
                        saga_instance.saga_id.to_string(),
                    ),
                ]);
                let request_id = match func(saga_data, &saga_headers, self.connection) {
                    Ok(request_id) => request_id,
                    Err(err) => {
                        return (saga_instance, Err(err));
                    }
                };
                saga_instance.last_request_id = Some(request_id);
                return (saga_instance, Ok(()));
            } else {
                i += direction;
                continue;
            }
        }
        saga_instance.end_state = true;
        return (saga_instance, Ok(()));
    }
}
