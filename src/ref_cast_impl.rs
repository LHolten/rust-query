/// Please don't use this macro, use `#[derive(RefCast)]` instead.
/// This macro is only safe whenever `#[derive(RefCast)]` also works.
/// The reason this macro exists is to not add a dependency on the `ref-cast` crate for all users of this library.
#[macro_export]
#[doc(hidden)]
macro_rules! unsafe_impl_ref_cast {
    ($name:ident) => {
        impl<T> $crate::private::RefCast for $name<T> {
            type From = T;

            #[inline]
            fn ref_cast(_from: &Self::From) -> &Self {
                unsafe { &*(_from as *const Self::From as *const Self) }
            }

            #[inline]
            fn ref_cast_mut(_from: &mut Self::From) -> &mut Self {
                unsafe { &mut *(_from as *mut Self::From as *mut Self) }
            }
        }
    };
}
