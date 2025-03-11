fn main() {
    tonic_build::configure()
        .compile_protos(
            &[
                "proto/command.proto",
                "proto/money.proto",
                "proto/restaurants.proto",
                "proto/consumers.proto",
                "proto/kitchens.proto",
            ],
            &["proto"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
