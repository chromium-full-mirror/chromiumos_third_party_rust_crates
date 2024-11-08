//! This library provides a derive macro for autogenerating getters. It can only be used on
//! named structs. It will generate getters that will reside in the struct namespace.
//!
//! # Derives
//!
//! Only named structs can derive `Getters`.
//!
//! # Methods generated
//!
//! The getter methods generated shall bear the same name as the struct fields and be
//! publicly visible. The methods return an immutable reference to the struct field of the
//! same name. If there is already a method defined with that name there'll be a collision.
//! In these cases one of two attributes can be set to either `skip` or `rename` the getter.
//! 
//!
//! # Usage
//!
//! In lib.rs or main.rs;
//!
//! ```edition2018
//! use derive_getters::Getters;
//!
//! #[derive(Getters)]
//! struct Number {
//!     num: u64,    
//! }
//! 
//! fn main() {
//!     let number = Number { num: 655 };
//!     assert!(number.num() == &655);
//! }
//! ```
//!
//! Here, a method called `num()` has been created for the `Number` struct which gives a
//! reference to the `num` field.
//!
//! This macro can also derive on structs that have simple generic types. For example;
//!
//! ```edition2018
//! # use derive_getters::Getters;
//! #[derive(Getters)]
//! struct Generic<T, U> {
//!     gen_t: T,
//!     gen_u: U,
//! }
//! #
//! # fn main() { }
//! ```
//!
//! The macro can also handle generic types with trait bounds. For example;
//! ```edition2018
//! # use derive_getters::Getters;
//! #[derive(Getters)]
//! struct Generic<T: Clone, U: Copy> {
//!     gen_t: T,
//!     gen_u: U,
//! }
//! #
//! # fn main() { }
//! ```
//! The trait bounds can also be declared in a `where` clause.
//!
//! Additionaly, simple lifetimes are OK too;
//! ```edition2018
//! # use derive_getters::Getters;
//! #[derive(Getters)]
//! struct Annotated<'a, 'b, T> {
//!     stuff: &'a T,
//!     comp: &'b str,
//!     num: u64,
//! }
//! #
//! # fn main() { }
//! ```
//!
//! # Attributes
//! Getters can be further configured to either skip or rename a getter.
//!
//! * #[getter(skip)]
//! Will skip generating a getter for the field being decorated.
//!
//! * #[getter(rename = "name")]
//! Changes the name of the getter (default is the field name) to "name".
//!
//!```edition2018
//! # use derive_getters::Getters;
//! #[derive(Getters)]
//! struct Attributed {
//!     keep_me: u64,
//!
//!     #[getter(skip)]
//!     skip_me: u64,
//!
//!     #[getter(rename = "number")]
//!     rename_me: u64,
//! }
//! #
//! # fn main() { }
//! ```
//!
//! # Panics
//!
//! If getters are derived on unit structs, unnamed structs, enums or unions.
//!
//! # Cannot Do
//! Const generics aren't handled by this macro nor are they tested.
use std::convert::TryFrom;

extern crate proc_macro;
use syn::{DeriveInput, parse_macro_input};

mod faultmsg;
mod getters;

/// Generate getter methods for all named struct fields in a seperate struct `impl` block.
/// Getter methods share the name of the field they're 'getting'. Methods return an
/// immutable reference to the field.
#[proc_macro_derive(Getters, attributes(getter))]
pub fn getters(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    
    getters::NamedStruct::try_from(&ast)
        .map(|ns| ns.emit())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
