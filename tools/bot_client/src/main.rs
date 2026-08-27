fn main() {
    let _ = (purgatory_common::version(), purgatory_protocol::version());
    println!("PURGATORY bot client bootstrap OK");
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_protocol::version().is_empty());
    }
}
