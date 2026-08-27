fn main() {
    let _ = (purgatory_common::version(), purgatory_content::version());
    println!("PURGATORY content validator bootstrap OK");
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_content::version().is_empty());
    }
}
