pub fn add(left: u64, right: u64) -> u64 {
    let mut sum = left;
    sum += right;
    sum
}

mod impostor {
    fn add() {}
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
