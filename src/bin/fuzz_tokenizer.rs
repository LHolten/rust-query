fn main() {
    #[cfg(fuzzing)]
    rust_query::fuzz_tokenizer();
}
