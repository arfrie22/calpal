pub mod calendar;
pub mod event;
pub mod timezone;   
pub mod types;

pub enum Error {
    InvalidDate,
    InvalidTime,
    InvalidDateTime,
    InvalidTimeRange,
    InvalidTimezone,
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
