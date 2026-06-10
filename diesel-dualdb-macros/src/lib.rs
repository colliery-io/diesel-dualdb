//! Procedural macros for [`diesel-dualdb`](https://docs.rs/diesel-dualdb).
//!
//! These are re-exported from the `diesel-dualdb` crate; depend on that, not on
//! this crate directly. Use them as `#[diesel_dualdb::test(...)]`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, ItemFn, LitStr, Path, Token};

/// Run one test body against each backend.
///
/// Apply to a `fn name(conn: &mut DualConnection) { … }`. It keeps your function
/// and adds a `#[test]` per selected backend, each constructing a
/// `DualConnection` and calling your body:
///
/// - `name_sqlite` — an in-memory `SqliteConnection`.
/// - `name_pg` — a `PgConnection` from `DUALDB_PG_URL`; if that env var is
///   unset the test prints a skip notice and returns.
///
/// Select backends with `#[diesel_dualdb::test(sqlite)]`,
/// `#[diesel_dualdb::test(pg)]`, or both. No arguments means both.
///
/// ```ignore
/// #[diesel_dualdb::test(pg, sqlite)]
/// fn round_trips(conn: &mut DualConnection) {
///     // identical assertions; both backends must pass
/// }
/// ```
///
/// Attributes you put on the function (e.g. `#[should_panic]`) are forwarded to
/// the generated test functions.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let (pg, sqlite) = match parse_backends(attr) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let func = parse_macro_input!(item as ItemFn);
    let name = func.sig.ident.clone();

    // The user's attributes (doc, #[should_panic], …) belong on the generated
    // `#[test]` fns, not on the inner helper.
    let user_attrs = func.attrs.clone();
    let mut inner = func;
    inner.attrs.clear();

    let mut out = quote! { #inner };

    if sqlite {
        let wname = format_ident!("{}_sqlite", name);
        out.extend(quote! {
            #(#user_attrs)*
            #[test]
            fn #wname() {
                let mut __dualdb_conn = ::diesel_dualdb::DualConnection::Sqlite(
                    <::diesel::SqliteConnection as ::diesel::Connection>::establish(":memory:")
                        .expect("dualdb::test: open in-memory sqlite"),
                );
                #name(&mut __dualdb_conn);
            }
        });
    }

    if pg {
        let wname = format_ident!("{}_pg", name);
        out.extend(quote! {
            #(#user_attrs)*
            #[test]
            fn #wname() {
                let __dualdb_url = match ::std::env::var("DUALDB_PG_URL") {
                    ::std::result::Result::Ok(u) => u,
                    ::std::result::Result::Err(_) => {
                        ::std::eprintln!(
                            "dualdb::test: DUALDB_PG_URL not set — skipping {}",
                            ::core::stringify!(#wname),
                        );
                        return;
                    }
                };
                let mut __dualdb_conn = ::diesel_dualdb::DualConnection::Pg(
                    <::diesel::PgConnection as ::diesel::Connection>::establish(&__dualdb_url)
                        .expect("dualdb::test: connect to postgres"),
                );
                #name(&mut __dualdb_conn);
            }
        });
    }

    out.into()
}

/// Generate the `MultiBackend` bridge impls for a **non-generic** portable type.
///
/// `bridge!(sql_type_marker, rust_newtype)` emits the three impls that make the
/// type work on one arm through `DualConnection` — exactly the hand-written
/// `mod <t>` blocks, in one line:
///
/// ```ignore
/// diesel_dualdb::bridge!(crate::sql_types::Uuid, crate::types::Uuid);
/// ```
///
/// It requires the per-backend `ToSql`/`FromSql<_, Pg/Sqlite>` for the type to
/// already exist (they do, alongside each type). Gate the call with the same
/// `#[cfg(feature = …)]` as the type itself.
///
/// Generic newtypes (e.g. `Json<T>`, whose `MultiBackend` `ToSql` needs
/// `T: Send + Sync + 'static`) are **not** covered — keep those hand-written.
#[proc_macro]
pub fn bridge(input: TokenStream) -> TokenStream {
    let Bridge { sql, rust } = parse_macro_input!(input as Bridge);
    quote! {
        impl ::diesel::sql_types::HasSqlType<#sql> for ::diesel_dualdb::MultiBackend {
            fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
                <::diesel_dualdb::MultiBackend>::lookup_sql_type::<#sql>(lookup)
            }
        }

        impl ::diesel::serialize::ToSql<#sql, ::diesel_dualdb::MultiBackend> for #rust {
            fn to_sql<'__b>(
                &'__b self,
                out: &mut ::diesel::serialize::Output<'__b, '_, ::diesel_dualdb::MultiBackend>,
            ) -> ::diesel::serialize::Result {
                out.set_value((#sql, self));
                ::core::result::Result::Ok(::diesel::serialize::IsNull::No)
            }
        }

        impl ::diesel::deserialize::FromSql<#sql, ::diesel_dualdb::MultiBackend> for #rust {
            fn from_sql(
                raw: <::diesel_dualdb::MultiBackend as ::diesel::backend::Backend>::RawValue<'_>,
            ) -> ::diesel::deserialize::Result<Self> {
                raw.from_sql::<Self, #sql>()
            }
        }
    }
    .into()
}

/// Derive a portable enum: a fieldless Rust enum that maps to a PostgreSQL
/// native `enum` type and to SQLite `TEXT` (the variant label), working through
/// `DualConnection` on one arm.
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, PartialEq, diesel_dualdb::DualEnum)]
/// #[dualdb(pg_type = "mood")]
/// pub enum Mood {
///     Happy,
///     Sad,
///     #[dualdb(rename = "meh")]
///     Neutral,
/// }
/// ```
///
/// This generates a marker SQL type `MoodSqlType` (use it in `table!`:
/// `feeling -> MoodSqlType`) and all the `ToSql`/`FromSql`/`AsExpression`/
/// `Queryable` impls plus the `MultiBackend` bridge. The Rust enum must derive
/// `Debug` (`ToSql` requires it) and should derive `Clone`.
///
/// - `#[dualdb(pg_type = "name")]` — the PostgreSQL enum type name (default: the
///   enum name lowercased). Your migration must `CREATE TYPE name AS ENUM (...)`
///   on Postgres; on SQLite the column is `TEXT`.
/// - `#[dualdb(rename = "label")]` on a variant — the stored label (default: the
///   variant identifier).
#[proc_macro_derive(DualEnum, attributes(dualdb))]
pub fn dual_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match dual_enum_impl(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn dual_enum_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let enum_ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            enum_ident,
            "DualEnum can only be derived for enums",
        ));
    };

    let mut pg_type = enum_ident.to_string().to_lowercase();
    for attr in &input.attrs {
        if attr.path().is_ident("dualdb") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("pg_type") {
                    pg_type = meta.value()?.parse::<LitStr>()?.value();
                    Ok(())
                } else {
                    Err(meta.error("unknown `dualdb` option (expected `pg_type`)"))
                }
            })?;
        }
    }

    // (variant ident, stored label)
    let mut variants: Vec<(Ident, String)> = Vec::new();
    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                v,
                "DualEnum variants must be unit (fieldless)",
            ));
        }
        let mut label = v.ident.to_string();
        for attr in &v.attrs {
            if attr.path().is_ident("dualdb") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        label = meta.value()?.parse::<LitStr>()?.value();
                        Ok(())
                    } else {
                        Err(meta.error("unknown `dualdb` option (expected `rename`)"))
                    }
                })?;
            }
        }
        variants.push((v.ident.clone(), label));
    }
    if variants.is_empty() {
        return Err(syn::Error::new_spanned(
            enum_ident,
            "DualEnum needs at least one variant",
        ));
    }

    let marker = format_ident!("{}SqlType", enum_ident);
    let to_label = variants
        .iter()
        .map(|(id, label)| quote!(#enum_ident::#id => #label));
    let from_label = variants
        .iter()
        .map(|(id, label)| quote!(#label => ::core::result::Result::Ok(#enum_ident::#id)));
    let from_label2 = variants
        .iter()
        .map(|(id, label)| quote!(#label => ::core::result::Result::Ok(#enum_ident::#id)));

    Ok(quote! {
        #[derive(
            ::diesel::sql_types::SqlType,
            ::diesel::query_builder::QueryId,
            ::core::clone::Clone,
            ::core::marker::Copy,
            ::core::fmt::Debug,
        )]
        #[diesel(postgres_type(name = #pg_type))]
        #[diesel(sqlite_type(name = "Text"))]
        pub struct #marker;

        const _: () = {
            fn label(value: &#enum_ident) -> &'static str {
                match value { #(#to_label,)* }
            }

            // ----- PostgreSQL: native enum (label on the wire) -----
            impl ::diesel::serialize::ToSql<#marker, ::diesel::pg::Pg> for #enum_ident {
                fn to_sql<'__b>(
                    &'__b self,
                    out: &mut ::diesel::serialize::Output<'__b, '_, ::diesel::pg::Pg>,
                ) -> ::diesel::serialize::Result {
                    ::std::io::Write::write_all(out, label(self).as_bytes())?;
                    ::core::result::Result::Ok(::diesel::serialize::IsNull::No)
                }
            }
            impl ::diesel::deserialize::FromSql<#marker, ::diesel::pg::Pg> for #enum_ident {
                fn from_sql(value: ::diesel::pg::PgValue<'_>) -> ::diesel::deserialize::Result<Self> {
                    match ::std::str::from_utf8(value.as_bytes())? {
                        #(#from_label,)*
                        other => ::core::result::Result::Err(
                            ::std::format!("unrecognized `{}` enum label: {:?}", #pg_type, other).into(),
                        ),
                    }
                }
            }

            // ----- SQLite: label as TEXT -----
            impl ::diesel::serialize::ToSql<#marker, ::diesel::sqlite::Sqlite> for #enum_ident {
                fn to_sql<'__b>(
                    &'__b self,
                    out: &mut ::diesel::serialize::Output<'__b, '_, ::diesel::sqlite::Sqlite>,
                ) -> ::diesel::serialize::Result {
                    out.set_value(::std::string::String::from(label(self)));
                    ::core::result::Result::Ok(::diesel::serialize::IsNull::No)
                }
            }
            impl ::diesel::deserialize::FromSql<#marker, ::diesel::sqlite::Sqlite> for #enum_ident {
                fn from_sql(
                    value: ::diesel::sqlite::SqliteValue<'_, '_, '_>,
                ) -> ::diesel::deserialize::Result<Self> {
                    let s = <::std::string::String as ::diesel::deserialize::FromSql<
                        ::diesel::sql_types::Text,
                        ::diesel::sqlite::Sqlite,
                    >>::from_sql(value)?;
                    match s.as_str() {
                        #(#from_label2,)*
                        other => ::core::result::Result::Err(
                            ::std::format!("unrecognized `{}` enum label: {:?}", #pg_type, other).into(),
                        ),
                    }
                }
            }

            // ----- MultiBackend bridge -----
            impl ::diesel::sql_types::HasSqlType<#marker> for ::diesel_dualdb::MultiBackend {
                fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
                    <::diesel_dualdb::MultiBackend>::lookup_sql_type::<#marker>(lookup)
                }
            }
            impl ::diesel::serialize::ToSql<#marker, ::diesel_dualdb::MultiBackend> for #enum_ident {
                fn to_sql<'__b>(
                    &'__b self,
                    out: &mut ::diesel::serialize::Output<'__b, '_, ::diesel_dualdb::MultiBackend>,
                ) -> ::diesel::serialize::Result {
                    out.set_value((#marker, self));
                    ::core::result::Result::Ok(::diesel::serialize::IsNull::No)
                }
            }
            impl ::diesel::deserialize::FromSql<#marker, ::diesel_dualdb::MultiBackend> for #enum_ident {
                fn from_sql(
                    raw: <::diesel_dualdb::MultiBackend as ::diesel::backend::Backend>::RawValue<'_>,
                ) -> ::diesel::deserialize::Result<Self> {
                    raw.from_sql::<Self, #marker>()
                }
            }

            // ----- Expression / Queryable wiring -----
            impl ::diesel::expression::AsExpression<#marker> for #enum_ident {
                type Expression = ::diesel::internal::derives::as_expression::Bound<#marker, Self>;
                fn as_expression(self) -> Self::Expression {
                    ::diesel::internal::derives::as_expression::Bound::new(self)
                }
            }
            impl ::diesel::expression::AsExpression<::diesel::sql_types::Nullable<#marker>> for #enum_ident {
                type Expression =
                    ::diesel::internal::derives::as_expression::Bound<::diesel::sql_types::Nullable<#marker>, Self>;
                fn as_expression(self) -> Self::Expression {
                    ::diesel::internal::derives::as_expression::Bound::new(self)
                }
            }
            impl<'__expr> ::diesel::expression::AsExpression<#marker> for &'__expr #enum_ident {
                type Expression = ::diesel::internal::derives::as_expression::Bound<#marker, Self>;
                fn as_expression(self) -> Self::Expression {
                    ::diesel::internal::derives::as_expression::Bound::new(self)
                }
            }
            impl<'__expr> ::diesel::expression::AsExpression<::diesel::sql_types::Nullable<#marker>>
                for &'__expr #enum_ident
            {
                type Expression =
                    ::diesel::internal::derives::as_expression::Bound<::diesel::sql_types::Nullable<#marker>, Self>;
                fn as_expression(self) -> Self::Expression {
                    ::diesel::internal::derives::as_expression::Bound::new(self)
                }
            }
            impl<__DB> ::diesel::deserialize::Queryable<#marker, __DB> for #enum_ident
            where
                __DB: ::diesel::backend::Backend,
                Self: ::diesel::deserialize::FromSql<#marker, __DB>,
            {
                type Row = Self;
                fn build(row: Self::Row) -> ::diesel::deserialize::Result<Self> {
                    ::core::result::Result::Ok(row)
                }
            }
        };
    })
}

/// `bridge!(Marker, Newtype)` input.
struct Bridge {
    sql: Path,
    rust: Path,
}

impl Parse for Bridge {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let sql = input.parse()?;
        input.parse::<Token![,]>()?;
        let rust = input.parse()?;
        // tolerate a trailing comma
        let _ = input.parse::<Option<Token![,]>>();
        Ok(Bridge { sql, rust })
    }
}

/// Parse the attribute arguments into `(pg, sqlite)` selections. Empty = both.
fn parse_backends(attr: TokenStream) -> syn::Result<(bool, bool)> {
    if attr.is_empty() {
        return Ok((true, true));
    }
    let idents = Punctuated::<Ident, Token![,]>::parse_terminated.parse(attr)?;
    let (mut pg, mut sqlite) = (false, false);
    for id in idents {
        match id.to_string().as_str() {
            "pg" => pg = true,
            "sqlite" => sqlite = true,
            _ => return Err(syn::Error::new(id.span(), "expected `pg` and/or `sqlite`")),
        }
    }
    Ok((pg, sqlite))
}
