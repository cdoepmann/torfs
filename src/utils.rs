//! Collection of useful helper code.

use std::collections::HashMap;

pub trait RetainOrElseVec {
    type Item;

    /// Retains only the elements specified by the predicate and calls another functor
    /// for the remove elements, for side-effects.
    fn retain_or_else<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::Item) -> bool,
        G: FnMut(&Self::Item);

    /// Remove the elements specified by the predicate and call another functor
    /// for them, for side-effects.
    fn remove_if<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::Item) -> bool,
        G: FnMut(&Self::Item),
    {
        let mut f = f;
        RetainOrElseVec::retain_or_else(self, |x| !f(x), g)
    }
}

impl<T> RetainOrElseVec for Vec<T> {
    type Item = T;

    fn retain_or_else<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::Item) -> bool,
        G: FnMut(&Self::Item),
    {
        let mut f = f;
        let mut g = g;

        self.retain(|x| match f(x) {
            true => true,
            false => {
                g(x);
                false
            }
        })
    }
}

pub trait RetainOrElseHashMap {
    type K;
    type V;

    /// Retains only the elements specified by the predicate and calls another functor
    /// for the remove elements, for side-effects.
    fn retain_or_else<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::K, &Self::V) -> bool,
        G: FnMut(&Self::K, &Self::V);
}

impl<K, V, H> RetainOrElseHashMap for HashMap<K, V, H> {
    type K = K;
    type V = V;

    fn retain_or_else<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::K, &Self::V) -> bool,
        G: FnMut(&Self::K, &Self::V),
    {
        let mut f = f;
        let mut g = g;

        self.retain(|k, v| match f(k, v) {
            true => true,
            false => {
                g(k, v);
                false
            }
        })
    }
}
