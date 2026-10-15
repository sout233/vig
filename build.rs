fn main() {
    println!("cargo:rerun-if-changed=src/asr/native.cpp");
    let mut build = cc::Build::new();
    build.cpp(true).file("src/asr/native.cpp");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Match the existing esaxx/tokenizer native dependency's MSVC runtime.
        build.flag("/EHsc").static_crt(true);
    } else {
        build.flag("-fexceptions");
    }
    build.compile("vig_asr_native");
}
