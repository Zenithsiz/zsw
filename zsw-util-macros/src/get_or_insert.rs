//! `GetOrInsert`

use {
	app_error::{AppError, Context, bail},
	convert_case::Casing,
	proc_macro2::TokenStream,
	quote::quote,
};

pub fn derive(input: proc_macro::TokenStream) -> Result<TokenStream, AppError> {
	let input: syn::DeriveInput = syn::parse(input).context("Unable to parse input")?;

	let syn::Data::Enum(data) = input.data else {
		bail!("GetOrInsert can only be derived for enums")
	};

	let enum_ident = &input.ident;

	let variants = data
		.variants
		.iter()
		.filter_map(|variant| Variant::try_from_syn_variant(enum_ident, variant).transpose())
		.collect::<Result<Vec<_>, _>>()?;

	Ok(quote! {
		#(#variants)*
	})
}

struct Variant<'a> {
	/// Enum identifier
	enum_ident: &'a syn::Ident,

	/// Variant identifier
	variant_ident: &'a syn::Ident,

	/// The type we're wrapping over
	field: &'a syn::Type,
}

impl<'a> Variant<'a> {
	fn try_from_syn_variant(enum_ident: &'a syn::Ident, variant: &'a syn::Variant) -> Result<Option<Self>, AppError> {
		let variant_ident = &variant.ident;

		let field = match &variant.fields {
			syn::Fields::Unit => return Ok(None),
			syn::Fields::Named(_) => bail!("{enum_ident}::{variant_ident}: GetOrInsert does not support named fields"),
			syn::Fields::Unnamed(fields_unnamed) => {
				let mut items_iter = fields_unnamed.unnamed.iter();
				let Some(first_field) = items_iter.next() else {
					bail!("{enum_ident}::{variant_ident}: GetOrInsert does not support empty tuple fields");
				};
				if items_iter.next().is_some() {
					bail!("{enum_ident}::{variant_ident}: GetOrInsert does not support multiple tuple fields");
				}

				&first_field.ty
			},
		};

		Ok(Some(Self {
			enum_ident,
			variant_ident,
			field,
		}))
	}
}

impl quote::ToTokens for Variant<'_> {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let Self {
			enum_ident,
			variant_ident,
			field,
		} = *self;

		let ident_snake = variant_ident.to_string().to_case(convert_case::Case::Snake);
		let insert_field = syn::Ident::new(&format!("insert_{ident_snake}"), enum_ident.span());
		let field_or_insert_default = syn::Ident::new(&format!("{ident_snake}_or_insert_default"), enum_ident.span());
		let impl_: syn::ItemImpl = syn::parse_quote! {
			impl #enum_ident {
				pub fn #insert_field(&mut self, value: #field) -> &mut #field {
					*self = #enum_ident::#variant_ident(value);
					match self {
						#enum_ident::#variant_ident(inner) => inner,
						_ => unreachable!(),
					}
				}

				pub fn #field_or_insert_default(&mut self) -> &mut #field
				where
					#field: Default
				{
					match self {
						#enum_ident::#variant_ident(inner) => inner,
						_ => self.#insert_field(<#field>::default()),
					}
				}
			}
		};

		impl_.to_tokens(tokens)
	}
}
