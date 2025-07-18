pub mod common {
    tonic::include_proto!("me.jangjunha.ftgo.common");
}

pub mod auth_service {
    tonic::include_proto!("me.jangjunha.ftgo.auth_service");
}

pub mod restaurant_service {
    tonic::include_proto!("me.jangjunha.ftgo.restaurant_service");
}

pub mod consumer_service {
    tonic::include_proto!("me.jangjunha.ftgo.consumer_service");
}

pub mod kitchen_service {
    tonic::include_proto!("me.jangjunha.ftgo.kitchen_service");
}

pub mod delivery_service {
    tonic::include_proto!("me.jangjunha.ftgo.delivery_service");
}

pub mod accounting_service {
    tonic::include_proto!("me.jangjunha.ftgo.accounting_service");
}

pub mod order_service {
    tonic::include_proto!("me.jangjunha.ftgo.order_service");
}
