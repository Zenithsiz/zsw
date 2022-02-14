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
	let mut func = syn::parse_macro_input!(item as syn::ItemFn);


	// Wrap the return type
	func.sig.output = {
		// Get the `->` and return type
		let (r_arrow, return_ty) = match func.sig.output {
			syn::ReturnType::Default =>
				proc_macro_error::abort!(func.sig.output, "Cannot use an empty output (yet), add `-> ()`"),
			syn::ReturnType::Type(r_arrow, ty) => (r_arrow, ty),
		};

		// Then wrap them in out side effect
		syn::ReturnType::Type(
			r_arrow,
			syn::parse_quote!(::zsw_util::WithSideEffect<#return_ty, (#effects)>),
		)
	};

	// Wrap the body
	// TODO: Deal with async, unsafe and what not.
	func.block = {
		let fn_body = func.block;

		// Check if we need to wrap async
		let wrapped_body: syn::Expr = match func.sig.asyncness.is_some() {
			true => syn::parse_quote! { move || async move { #fn_body } },
			false => syn::parse_quote! { move || { #fn_body } },
		};
		let output_expr: syn::Expr = match func.sig.asyncness.is_some() {
			true => syn::parse_quote! { ::zsw_util::WithSideEffect::new((#wrapped_body)().await) },
			false => syn::parse_quote! { ::zsw_util::WithSideEffect::new((#wrapped_body)()) },
		};


		syn::parse_quote! { {#output_expr} }
	};

	quote::quote! {
		#func
	}
	.into()
}
