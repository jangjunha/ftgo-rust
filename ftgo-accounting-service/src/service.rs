use bigdecimal::BigDecimal;
use eventstore::ExpectedRevision;
use uuid::Uuid;

use crate::{models, store::AccountStore};

pub struct AccountingService {
    store: AccountStore,
}

impl AccountingService {
    pub async fn create(
        &self,
        account_id: Option<Uuid>,
    ) -> Result<models::Account, AccountingError> {
        let account_id = account_id.unwrap_or_else(|| Uuid::new_v4());
        let account = models::Account::new(account_id);

        let event = account.open().map_err(|_| AccountingError::Internal)?;

        let _ = self
            .store
            .append(
                &account_id,
                &vec![(None, event.clone())],
                ExpectedRevision::NoStream,
            )
            .await;

        Ok(account.apply(event))
    }

    pub async fn deposit(
        &self,
        account_id: Uuid,
        amount: BigDecimal,
        description: Option<String>,
        event_id: Option<Uuid>,
    ) -> Result<models::Account, AccountingError> {
        let account = self
            .store
            .get(&account_id)
            .await
            .map_err(|_| AccountingError::Internal)?;

        // TODO: process saga reply event
        let events = match account.deposit(amount, description) {
            Ok(event) => vec![(event_id, event)],
            Err(_) => vec![],
        };

        let _ = self
            .store
            .append(&account_id, &events, ExpectedRevision::StreamExists)
            .await;

        Ok(events.into_iter().fold(account, |acc, (_, e)| acc.apply(e)))
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: BigDecimal,
        description: Option<String>,
        event_id: Option<Uuid>,
    ) -> Result<models::Account, AccountingError> {
        let account = self
            .store
            .get(&account_id)
            .await
            .map_err(|_| AccountingError::Internal)?;

        // TODO: process saga reply event
        let events = match account.withdraw(amount, description) {
            Ok(event) => vec![(event_id, event)],
            Err(_) => vec![],
        };

        let _ = self
            .store
            .append(&account_id, &events, ExpectedRevision::StreamExists)
            .await;

        Ok(events.into_iter().fold(account, |acc, (_, e)| acc.apply(e)))
    }

    // TODO: get account

    // TODO: list account
}

impl Default for AccountingService {
    fn default() -> Self {
        Self {
            store: AccountStore::default(),
        }
    }
}

pub enum AccountingError {
    Internal,
}
