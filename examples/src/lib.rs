#[cfg(test)]
mod test {
    pub fn foo() {}

    #[test]
    fn test() {
        foo();
    }
}
