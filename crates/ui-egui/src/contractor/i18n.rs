//! Extension catalogs stay beside the adapter; upstream catalog entries take precedence.
//! Ukrainian is CADCraft's required reference locale. Other locales retain its English fallback.

const UK: &[(&str, &str)] = &[
    ("Contractor", "Будівництво"),
    ("Framing Tool Settings", "Налаштування каркаса"),
    ("Place Lumber", "Розмістити пиломатеріали"),
    ("Place Post / Piling", "Розмістити стійку / палю"),
    ("Convert Selected Lines / Circles", "Перетворити вибрані лінії / кола"),
    ("Repeat at On-center Spacing", "Повторити з міжосьовим кроком"),
    ("Edit Framing Member", "Редагувати елемент каркаса"),
    ("Framing Members", "Елементи каркаса"),
    ("Framing View", "Вигляд каркаса"),
    ("Plan View", "Вигляд у плані"),
    ("Engineering View", "Інженерний вигляд"),
    ("3D View", "3D-вигляд"),
    ("Export Framing Drawing", "Експортувати креслення каркаса"),
    ("Sample Floor Framing", "Приклад каркаса перекриття"),
];

pub(crate) fn entries(language: &str) -> &'static [(&'static str, &'static str)] {
    match language {
        "uk" => UK,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_catalog_is_unique_and_reaches_the_host_translator() {
        let mut seen = std::collections::HashSet::new();
        for &(key, value) in UK {
            assert!(seen.insert(key));
            assert!(!value.is_empty());
            assert_eq!(crate::i18n::tr("uk", key), value);
        }
        assert_eq!(crate::i18n::tr("en", "Place Lumber"), "Place Lumber");
    }
}
