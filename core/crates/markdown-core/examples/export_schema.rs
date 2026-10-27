fn main() {
    let schema = markdown_core::document_schema();
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("schema serialization")
    );
}
