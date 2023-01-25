//! Collection of useful helper code.

pub trait RetainOrElse {
    type Item;

    /// Retains only the elements specified by the predicate and calls another functor
    /// for the remove elements, for side-effects.
    fn retain_or_else<F, G>(&mut self, f: F, g: G)
    where
        F: FnMut(&Self::Item) -> bool,
        G: FnMut(&Self::Item);
}

impl<T> RetainOrElse for Vec<T> {
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
