pub(super) fn items(worn: Option<usize>, schemes: &[omsi_sim::vehicle::PaintScheme]) -> Vec<(String, String)> {
    let mark = |on: bool, name: String| if on { format!("● {name}") } else { name };
    let mut out = vec![(mark(worn.is_none(), omsi_ui::tr("Default paint").into_owned()), "livery -1".to_string())];
    out.extend(schemes.iter().enumerate().map(|(i, s)| (mark(worn == Some(i), s.name.clone()), format!("livery {i}"))));
    out
}

pub(super) fn chosen(arg: &str) -> Option<Option<usize>> {
    arg.trim().parse::<i64>().ok().map(|i| usize::try_from(i).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_paint_comes_first_and_the_worn_one_is_marked() {
        let schemes: Vec<_> = ["BVG", "InterEssonne"].into_iter().map(|n| omsi_sim::vehicle::PaintScheme { name: n.into(), ..Default::default() }).collect();
        let lines = items(Some(1), &schemes);
        let actions: Vec<_> = lines.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(actions, ["livery -1", "livery 0", "livery 1"]);
        assert_eq!(lines[1].0, "BVG");
        assert_eq!(lines[2].0, "● InterEssonne");
        assert!(!lines[0].0.starts_with("● "));
        assert!(items(None, &schemes)[0].0.starts_with("● "));
        assert_eq!((chosen(" 1"), chosen("-1"), chosen("x")), (Some(Some(1)), Some(None), None));
    }
}
