//! Utility macros

// Features
#![feature(must_not_suspend, yeet_expr)]

mod get_or_insert;

use {proc_macro::TokenStream, quote::quote};

#[proc_macro_derive(GetOrInsert)]
pub fn derive_get_or_insert(input: TokenStream) -> TokenStream {
	match get_or_insert::derive(input) {
		Ok(output) => output,
		Err(err) => {
			let err = format!("{err:?}");
			quote! {
				compile_error!(#err);
			}
		},
	}
	.into()
}
