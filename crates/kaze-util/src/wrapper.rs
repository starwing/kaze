#[macro_export]
macro_rules! make_wrapper {
    ($name:ident) => {
        make_wrapper!($name, inner);
    };
    ($name:ident, $field:ident) => {
        impl<T: std::fmt::Debug> std::fmt::Debug for $name<T> {
            fn fmt(
                &self,
                f: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field(stringify!($field), &self.$field)
                    .finish()
            }
        }

        impl<T> AsRef<T> for $name<T> {
            fn as_ref(&self) -> &T {
                &self.$field
            }
        }

        impl<T> AsMut<T> for $name<T> {
            fn as_mut(&mut self) -> &mut T {
                &mut self.$field
            }
        }

        impl<T> std::ops::Deref for $name<T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                &self.$field
            }
        }

        impl<T> std::ops::DerefMut for $name<T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.$field
            }
        }
    };
}
