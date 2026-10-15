#[derive(Default)]
pub struct Counter {
    count: usize,
}

impl Counter {
    pub fn advance(&mut self) -> usize {
        self.count += 1;
        self.count - 1
    }
}
