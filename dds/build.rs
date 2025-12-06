use std::{
    fs::{self, File, read_dir},
    io::{self, Read, Write},
    path::Path,
    process::Command,
};

fn main() {
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
            let output_file = output_dir.join(&rust_filename);

            run_rtps_gen(&path, &output_file);
            add_new_methods_to_structs(&output_file).unwrap();
            modules.push(stem);
        }
    }

    generate_includes(src_dir, modules.as_slice());
}

fn run_rtps_gen(idl_path: &Path, output_path: &Path) {
    println!("cargo:info=Running rtps-gen for {idl_path:?}");

    let status = Command::new("rtps-gen")
        .arg(idl_path)
        .arg("-o")
        .arg(output_path)
        .status()
        .expect("Failed to run rtps-gen");

    if !status.success() {
        println!("cargo:error=rtps-gen failed on {idl_path:?}");
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

fn add_new_methods_to_structs(file: &Path) -> io::Result<()> {
    let mut content = String::new();
    File::open(file)?.read_to_string(&mut content)?;

    let lines: Vec<&str> = content.lines().collect();
    let mut out = String::new();

    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx];

        if line.contains("enum") {
            out.push_str("    #[derive(Copy)]\n");
        }

        if let Some(line) = const_string_to_str_slice(line) {
            out.push_str("#[allow(non_upper_case_globals)]\n");
            out.push_str(&line);
        } else {
            out.push_str(line);
        }

        out.push('\n');

        // Look for "struct <Name> {"
        if let Some(struct_name) = extract_struct_name(line) {
            // Collect field lines until "}"
            let mut fields = Vec::new();
            idx += 1;

            while idx < lines.len() {
                let struct_line = lines[idx];
                if struct_line.trim().starts_with('}') {
                    out.push_str(struct_line);
                    out.push('\n');
                    break;
                }

                if let Some((fname, ftype)) = extract_field(struct_line) {
                    fields.push((fname, ftype));
                }

                out.push_str(struct_line);
                out.push('\n');
                idx += 1;
            }

            // Now generate the impl block
            if !fields.is_empty() {
                out.push_str(&generate_impl(&struct_name, &fields));
            }
        }

        idx += 1;
    }

    fs::File::create(file)?.write_all(out.as_bytes())
}

fn extract_struct_name(line: &str) -> Option<String> {
    // normalize whitespace
    let trimmed = line.trim();

    // Only check lines containing "struct"
    if !trimmed.contains("struct") {
        return None;
    }

    // Split on whitespace
    let parts: Vec<&str> = trimmed.split_whitespace().collect();

    // Look for the "struct" keyword
    let pos = parts.iter().position(|p| *p == "struct")?;

    // Next token should be the struct name
    let name_pos = 1;
    if pos + name_pos >= parts.len() {
        return None;
    }

    let mut name = parts[pos + name_pos].to_string();

    // Remove trailing '{' if present (e.g. "Foo{")
    name = name.trim_matches('{').to_string().trim().to_string();

    if name.is_empty() { None } else { Some(name) }
}

fn extract_field(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.contains(':') {
        let mut parts = trimmed.split(':');
        let name = parts.next()?.trim().to_string();
        let rest = parts.next()?.trim();

        // remove trailing comma
        let ty = rest.trim_end_matches(',').trim().to_string();
        return Some((name, ty));
    }
    None
}

fn generate_impl(struct_name: &str, fields: &[(String, String)]) -> String {
    let mut params = String::new();
    let mut assigns = String::new();
    let mut getters = String::new();
    let mut setters = String::new();

    for (name, ty) in fields {
        params.push_str(&format!("{name}: {ty}, "));
        assigns.push_str(&format!("{name}, "));
        setters.push_str(&format!(
            "pub fn set_{name}(&mut self, value: {ty}) {{ self.{name} = value }}\n"
        ));
        if ty == "String" || ty.contains("Seq") || ty.contains("ts") {
            getters.push_str(&format!(
                "pub fn {name}(&self) -> &{ty} {{ &self.{name} }}\n"
            ));
        } else {
            getters.push_str(&format!("pub fn {name}(&self) -> {ty} {{ self.{name} }}\n"));
        }
        setters.push_str("    ");
        getters.push_str("    ");
    }

    format!(
        "
impl {struct_name} {{
    pub fn new({params}) -> Self {{
        Self {{ {assigns} }}
    }}
    {getters}
    {setters}
}}
"
    )
}

fn const_string_to_str_slice(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if !(trimmed.contains("const") && trimmed.contains("String")) {
        None
    } else if trimmed.contains("pub") {
        Some(trimmed.replace("String", "&str"))
    } else {
        let replaced_line = trimmed.replace("String", "&str");
        Some(format!("pub {replaced_line}"))
    }
}
