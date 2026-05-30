use sillyecs::EcsCode;
use std::fs::File;
use std::io::BufReader;

fn main() {
    println!("cargo:rerun-if-changed=ecs.yaml");
    let file = File::open("ecs.yaml").expect("open ecs.yaml");
    EcsCode::generate(BufReader::new(file))
        .expect("generate ECS")
        .write_files()
        .expect("write ECS files");
}
