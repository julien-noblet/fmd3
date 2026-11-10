fn main() {
    println!("cargo:rerun-if-changed=src/c/pb.c");
    println!("cargo:rerun-if-changed=src/c/pb.h");

    let mut build = cc::Build::new();
    build.file("src/c/pb.c");
    build.include("src/c/include");
    if let Ok(include_dir) = std::env::var("DEP_LUA_INCLUDE") {
        build.include(include_dir);
    }
    build.compile("lua_protobuf");
}
