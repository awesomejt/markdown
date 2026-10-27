fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&markdown_core::v2::document_schema()).unwrap()
    );
}
