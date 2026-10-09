use proc_macro::TokenStream;

#[proc_macro]
pub fn nothing(_input: TokenStream) -> TokenStream {
    shared::shared();
    pmonly::pmonly();
    TokenStream::new()
}
