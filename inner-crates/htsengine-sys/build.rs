use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

use http_req::request;
use sha2::{Digest, Sha256};
use unwrap::unwrap;

use flate2::read::GzDecoder;
use tar::Archive;

const SOURCE_BASE_URL: &str =
    "https://udomain.dl.sourceforge.net/project/hts-engine/hts_engine%20API";
const HTS_VERSION: &str = "1.10";
const SHA256: &str = "e2132be5860d8fb4a460be766454cfd7c3e21cf67b509c48e1804feab14968f7";

fn main() {
    let download_url = format!(
        "{url}/hts_engine_API-{ver}/hts_engine_API-{ver}.tar.gz",
        url = SOURCE_BASE_URL,
        ver = HTS_VERSION
    );
    let buffer = match fetch_source(&download_url) {
        Ok(cursor) => cursor,
        Err(e) => panic!("Failed to download htsengine: {}", e),
    };

    let mut install_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    install_dir.push("installed");
    unwrap!(fs::create_dir_all(&install_dir));

    let decoder = GzDecoder::new(buffer);
    let mut archive = Archive::new(decoder);
    let mut source_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    unwrap!(fs::create_dir_all(&source_dir));
    unwrap!(archive.unpack(&source_dir));
    source_dir.push(format!("hts_engine_API-{}", HTS_VERSION));
    
    build(&source_dir, &install_dir);

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", install_dir.to_string_lossy()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");
    let path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(path.join("bindings.rs"))
        .expect("couldnot write bindings");
}

fn fetch_source(url: &str) -> Result<Cursor<Vec<u8>>, String> {
    let mut buffer = Vec::new();
    let resp = request::get(url, &mut buffer).map_err(|e| e.to_string())?;
    if !resp.status_code().is_success() {
        return Err(format!("Download Error: HTTP {}", resp.status_code()));
    }
    let hash = format!("{:x}", Sha256::digest(&buffer));
    if &hash != SHA256 {
        return Err("Download source file failed hash check".to_string());
    }
    Ok(Cursor::new(buffer))
}

fn build(source_dir: &Path, install_dir: &Path) {
    let builder = cc::Build::new();
    let compiler = unwrap!(builder.get_compiler().path().to_str()).to_string();
    let mut cflags = env::var("CFLAGS").unwrap_or(String::default());
    cflags += " -O2";
    let mut configure = Command::new("./configure");
    if !compiler.is_empty() {
        configure.env("CC", &compiler);
    }
    configure.env("CFLAGS", &cflags);
    let result = configure
        .current_dir(&source_dir)
        .arg(&format!("--prefix={}", &install_dir.to_string_lossy()))
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run ./configure: {}", e);
        });
    if !result.status.success() {
        panic!(
            "\n{:?}\nCFLAGS={}\nCC={}\n{}\n{}\n",
            configure,
            cflags,
            compiler,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr),
        );
    }

    let mut make = Command::new("make");
    let j_arg = format!("-j{}", env::var("NUM_JOBS").unwrap_or("4".to_string()));
    let output = make
        .current_dir(&source_dir)
        .arg(&j_arg)
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run make: {}", e);
        });
    if !output.status.success() {
        panic!(
            "\n{:?}\nCFLAGS={}\nCC={}\n{}\n{}\n",
            configure,
            cflags,
            compiler,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let mut make = Command::new("make");
    let output = make
        .current_dir(&source_dir)
        .arg("install")
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run make: {}", e);
        });
    if !output.status.success() {
        panic!(
            "\n{:?}\nCFLAGS={}\nCC={}\n{}\n{}\n",
            configure,
            cflags,
            compiler,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    println!(
        "cargo:rustc-link-search=native={}/lib",
        install_dir.to_string_lossy()
    );
    println!("cargo:include={}/include", install_dir.to_string_lossy());
}
