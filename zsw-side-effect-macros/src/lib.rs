//! Side effect macros

// Imports
use proc_macro::TokenStream;

/// Adds a side effect to a function
#[proc_macro_error::proc_macro_error]
#[proc_macro_attribute]
pub fn side_effect(attr: TokenStream, item: TokenStream) -> TokenStream {
	// Get all effects as a type
	let effects = syn::parse_macro_input!(attr as syn::Type);

	// Then parse the function
	let func = syn::parse_macro_input!(item as syn::ItemFn);

	// Create the outer function by wrapping the output type
	let outer_func = {
		let mut outer_func = func.clone();
		// Wrap the return type
		let (r_arrow, return_ty) = match func.sig.output {
			syn::ReturnType::Default =>
				proc_macro_error::abort!(func.sig.output, "Cannot use an empty output (yet), add `-> ()`"),
			syn::ReturnType::Type(r_arrow, ty) => (r_arrow, ty),
		};
		outer_func.sig.output = syn::ReturnType::Type(
			r_arrow,
			syn::parse_quote!(::zsw_util::WithSideEffect<#return_ty, (#effects)>),
		);

		// Wrap the body
		// TODO: Deal with async, unsafe and what not.
		let inner_fn_body = func.block;
		outer_func.block = syn::parse_quote! {{
			let mut __inner_fn = move || { #inner_fn_body };
			::zsw_util::WithSideEffect::new(__inner_fn())
		}};

		outer_func
	};

	quote::quote! {
		#outer_func
	}
	.into()
}
