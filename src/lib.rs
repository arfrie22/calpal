pub mod calendar;
pub mod event;
pub mod timezone;   
pub mod types;

pub enum ICalTypes {
    Binary,
    Boolean,
    CalAddress,
    Date,
    DateTime,
    Duration,
    Float,
    Integer,
    Period,
    Recur,
    Text,
    Time,
    URI,
    UTCOffset,
}

pub enum Error {
    TypeDecode(ICalTypes),
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
