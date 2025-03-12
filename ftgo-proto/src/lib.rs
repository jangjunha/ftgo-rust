pub mod common {
    tonic::include_proto!("me.jangjunha.ftgo.common");
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
