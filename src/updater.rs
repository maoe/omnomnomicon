//! Interactive structure updater
//!
//! Library offers a way to update a structure using [`Updater`] and a macro to derive it.
//!
//! ```ignore
//! #[derive(Updater)]
//! struct Banana {
//!     shape: u32,
//!     color: f64
//! }
//! let mut banana = Banana { shape: 12, color: 3.1415 };
//! let p = updater_for(&banana, "banana");
//! let r = parse_result(&p, "banana . shape 16")?;
//! banana.apply(r);
//! // Banana { shape: 16, color: 3.1415 }
//! ```

use std::borrow::Cow;

use crate::prelude::*;
use crate::Result;

/// Entry point for interactive structure updated
///
/// ```ignore
/// #[derive(Updater)]
/// struct Banana {
///     shape: u32,
///     color: f64
/// }
/// let mut banana = Banana { shape: 12, color: 3.1415 };
/// let p = updater_for(&banana, "banana");
/// let r = parse_result(&p, "banana . shape 16")?;
/// banana.apply(r);
/// // Banana { shape: 16, color: 3.1415 }
/// ```
pub fn updater_for<'a, T>(
    item: &'a T,
    label: &'static str,
) -> impl FnMut(&str) -> Result<T::Updater> + 'a
where
    T: Updater,
{
    move |input| Updater::enter(item, label, input)
}

/// Interactive structure updater trait
///
/// This trait creates interactive update menu for a struct
/// See [`updater_for`] for examples
pub trait Updater {
    /// A separate type containing changes to one of the fields in `Self` such that [`Updater::apply`] properties hold.
    ///
    /// For example for a pair of `i32` `type Foo = (i32, i32)` an `Updater` is a `(bool, i32)`,
    /// where `bool` indicates if the `i32` value should replace the value in the first or the second
    /// field.
    ///
    /// Updater is a property of a new data, not field
    type Updater;

    /// Parse an updater for a current item
    ///
    /// For structures containing multiple fields first thing `enter` method shoud do is to
    /// parse `entry` as [`literal`] from the input and ignore it's value
    fn enter<'a>(&self, entry: &'static str, input: &'a str) -> Result<'a, Self::Updater>;

    /// Apply changes from [`Self::Updater`] to a current value
    fn apply(&mut self, updater: Self::Updater) -> core::result::Result<(), String>;
}

impl<T> Updater for T
where
    T: Parser + std::fmt::Debug,
{
    type Updater = T;
    fn enter<'a>(&self, _: &'static str, input: &'a str) -> Result<'a, Self::Updater> {
        with_hint(
            || Some(Cow::from(format!("cur: {:?}", self))),
            Parser::parse,
        )(input)
    }
    fn apply(&mut self, updater: Self::Updater) -> core::result::Result<(), String> {
        *self = updater;
        Ok(())
    }
}

impl<T: Parser + Clone + std::fmt::Debug> Updater for Option<T> {
    type Updater = Option<T>;

    fn enter<'a>(&self, _: &'static str, input: &'a str) -> Result<'a, Self::Updater> {
        let enabled = tagged("some", fmap(Some, T::parse));
        let disabled = tag(None, "none");

        with_hint(
            || Some(Cow::from(format!("cur: {:?}", self))),
            or(enabled, disabled),
        )(input)
    }

    fn apply(&mut self, updater: Self::Updater) -> core::result::Result<(), String> {
        *self = updater;
        Ok(())
    }
}

impl<T: Updater + std::fmt::Debug, const N: usize> Updater for [T; N] {
    type Updater = (usize, T::Updater);

    fn enter<'a>(&self, entry: &'static str, input: &'a str) -> Result<'a, Self::Updater> {
        let label_ix = || Some(Cow::from(format!("arr index, 0..{}", N - 1)));
        let parse_ix = with_hint(label_ix, number::<usize>);
        let (output, _) = literal(entry)(input)?;
        let key_input = output.input;
        let (output, key) = output.bind_space(true, parse_ix)?;
        if key >= N {
            return Terminate::fail(key_input, format!("Index too big, valid range 0..{}", N));
        }
        let (output, val) = output.bind_space(true, |i| self[key].enter(".", i))?;
        Ok((output, (key, val)))
    }

    fn apply(&mut self, updater: Self::Updater) -> core::result::Result<(), String> {
        self[updater.0].apply(updater.1)
    }
}

#[cfg(feature = "enum-map")]
impl<K, V> Updater for enum_map::EnumMap<K, V>
where
    K: enum_map::EnumArray<V> + Copy + std::fmt::Debug,
    V: Updater,
{
    type Updater = (K, <V as Updater>::Updater);

    fn enter<'a>(&self, entry: &'static str, input: &'a str) -> Result<'a, Self::Updater> {
        let table = self.iter().map(|(k, _)| (k, Cow::from(format!("{:?}", k))));
        let (output, _) = literal(entry)(input)?;
        let (output, key) = output.bind_space(true, lookup_key(table, 100))?;
        let (output, val) = output.bind_space(true, |i| self[key].enter(".", i))?;
        Ok((output, (key, val)))
    }

    fn apply(&mut self, (key, updater): Self::Updater) -> core::result::Result<(), String> {
        self[key].apply(updater)
    }
}

#[test]
fn test_updater() {
    mod foo {
        pub fn ten_percent(orig: &f64, new: &f64) -> std::result::Result<(), String> {
            let diff = (*orig - *new) * 100.0 / (*orig);
            if (-10.0..=10.0).contains(&diff) {
                Ok(())
            } else {
                Err(format!("Change {} -> {} is to large", orig, new))
            }
        }
    }

    fn positive(_: &f64, new: &f64) -> std::result::Result<(), String> {
        if *new > 0.0 {
            Ok(())
        } else {
            Err("New value must be positive".to_owned())
        }
    }
    pub use foo::ten_percent;

    #[derive(Debug, Updater)]
    struct Foo {
        #[om(check(foo::ten_percent))]
        foo: f64,
        #[om(check(ten_percent), check(positive))]
        bar: f64,
    }

    let mut payload = Foo {
        foo: 100.0,
        bar: 100.0,
    };

    payload.apply(FooUpdater::Foo(95.0)).unwrap();
    payload.apply(FooUpdater::Foo(65.0)).unwrap_err();

    payload.bar = -100.0;
    payload.apply(FooUpdater::Bar(-99.0)).unwrap_err();
}

// TODO HashMap
