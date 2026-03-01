use omg_idl_code_gen::{Configuration, generate_with_search_path};
use std::{
    fs::{File, read_dir},
    io::Write,
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Directory where your IDL files live
    let idl_dir = "idl";

    // Tell Cargo to rerun build.rs if any IDL file changes
    println!("cargo:rerun-if-changed={idl_dir}");
    println!("cargo:rerun-if-changed=build.rs");

    // Collect all .idl files from idl_dir
    let paths = read_dir(idl_dir).expect("Could not read idl/ directory");

    let output_dir = std::env::var("OUT_DIR").unwrap();
    let output_dir = std::path::Path::new(&output_dir);
    let src_dir = Path::new("src");

    let mut modules = Vec::new();
    for entry in paths {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("idl") {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let rust_filename = format!("{stem}.rs");
            let idl_filename = format!("{stem}.idl");
            let output_file = output_dir.join(&rust_filename);

            let out_file = File::create(output_file)?;
            let config = Configuration::new(Path::new(idl_dir), Path::new(&idl_filename), true);
            gen_rs_from_idl(config, &path, out_file);
            modules.push(stem);
        }
    }

    generate_includes(src_dir, modules.as_slice());
    Ok(())
}

fn gen_rs_from_idl(cfg: Configuration, idl_file: &Path, mut out_file: File) {
    println!("cargo:info=Generating Rust code for {:?}", idl_file);

    let res = generate_with_search_path(&mut out_file, &cfg);
    if let Err(e) = res {
        println!(
            "cargo:error=Failed to generate code for {:?}. Saw {e:#?}",
            idl_file
        );
    }
}

fn generate_includes(out_dir: &Path, modules: &[String]) {
    let generated_rs = out_dir.join("lib.rs");
    let mut file = File::create(&generated_rs).expect("Could not create lib.rs");

    for module in modules {
        // Convert module name to valid Rust identifier if needed
        writeln!(
            file,
            "include!(concat!(env!(\"OUT_DIR\"), \"/{module}.rs\"));"
        )
        .unwrap();
    }

    println!("cargo:info=Written {generated_rs:?}");
}
