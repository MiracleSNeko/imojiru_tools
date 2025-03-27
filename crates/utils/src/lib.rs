mod utils {
    use anyhow::{Result as AnyResult, anyhow};
    use std::error::Error as StdError;

    pub trait IntoAnyResult<T> {
        fn into_anyresult(self) -> AnyResult<T>;
    }

    impl<T> IntoAnyResult<T> for Option<T> {
        fn into_anyresult(self) -> AnyResult<T> {
            self.ok_or(anyhow!("called `Option::unwrap()` on a `None` value"))
        }
    }

    impl<T, E> IntoAnyResult<T> for Result<T, E>
    where
        E: StdError + Send + Sync + 'static,
    {
        fn into_anyresult(self) -> AnyResult<T> {
            self.map_err(|e| anyhow!(e))
        }
    }
}

pub use utils::*;
