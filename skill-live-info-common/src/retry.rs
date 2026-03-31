use std::future::Future;

pub async fn with_retries<T, E, F, Fut>(attempts: usize, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let attempts = attempts.max(1);
    let mut last_error = None;

    for index in 0..attempts {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                if index + 1 == attempts {
                    break;
                }
            }
        }
    }

    if let Some(error) = last_error {
        Err(error)
    } else {
        unreachable!("at least one attempt must run")
    }
}
