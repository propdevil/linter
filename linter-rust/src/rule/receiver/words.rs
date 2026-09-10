pub(super) fn words(value: &str) -> Vec<String> {
    let characters: Vec<char> = value.trim_start_matches("r#").chars().collect();
    let mut words = Vec::new();
    let mut start = 0;
    for index in 1..characters.len() {
        let previous = characters[index - 1];
        let current = characters[index];
        let next = characters.get(index + 1).copied();
        let boundary = !previous.is_ascii_alphanumeric()
            || !current.is_ascii_alphanumeric()
            || (previous.is_ascii_lowercase() && current.is_ascii_uppercase())
            || (previous.is_ascii_uppercase()
                && current.is_ascii_uppercase()
                && next.is_some_and(|next| next.is_ascii_lowercase()))
            || (previous.is_ascii_alphabetic() && current.is_ascii_digit())
            || (previous.is_ascii_digit() && current.is_ascii_alphabetic());
        if boundary {
            push_word(&characters[start..index], &mut words);
            start = if current.is_ascii_alphanumeric() {
                index
            } else {
                index + 1
            };
        }
    }
    push_word(&characters[start..], &mut words);
    words
}

fn push_word(characters: &[char], words: &mut Vec<String>) {
    let word: String = characters
        .iter()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect();
    if !word.is_empty() {
        words.push(word);
    }
}
