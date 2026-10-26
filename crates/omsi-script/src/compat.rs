//! Fixes for mod scripts that cannot show what they were written to show.
//!
//! Each fix is recognised by the script's own text, never by a file or bus name, and is
//! applied to the source lines before they are compiled.

/// The 4-character Annax line matrix of the LiAZ 5292 (a rewrite of the stock 3-character
/// `Matrix_D.osc`): a line number of one digit is first padded to four characters
/// (`4 $SetLengthL` → `"5   "`) and then cut to its last three (`3 $SetLengthR`, which
/// keeps the right end in OMSI too - see FORMATS.md), so line 5 came out blank and 5E as
/// `"   E"`. The number is written with three digits instead (`005E`, `051E`, `123E`), which
/// is what a three-digits-and-a-letter display shows.
///
/// The display has three cells for digits and a fourth for the letter, but the script
/// writes the letter lines Berlin style in front of the last digit or two (`" D" … 1
/// $SetLengthR " " $+ $+`): line 52D came out as `" D2 "`. Those lines are rewritten to
/// the number's three digits followed by the letter (`052D`), as the lines with a letter
/// behind the number (`10`, `30`…) already were.
fn four_char_matrix(lines: &mut [String]) -> bool {
    let has = |s: &str| lines.iter().any(|l| l.split_whitespace().collect::<Vec<_>>().join(" ") == s);
    if !(has("(L.$.Matrix_NewNr) $length 1 <=") && has("(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+") && has("4 $SetLengthL")) {
        return false;
    }
    let mut changed = false;
    // whatever the letter code made of it, the number stored for the display goes through
    // one rule at the end: digits first, the letter in the fourth cell
    let n = lines.len();
    for i in 0..n {
        if lines[i].split_whitespace().collect::<Vec<_>>().join(" ") != "4 $SetLengthR" {
            continue;
        }
        let next = lines[i + 1..].iter().map(|l| l.trim()).find(|l| !l.is_empty());
        if next == Some("(S.$.Matrix_NewNr)") {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!("{indent}4 $SetLengthR $__DigitsFirst");
            changed = true;
        }
    }
    for l in lines.iter_mut() {
        if l.split_whitespace().collect::<Vec<_>>().join(" ") == "l1 trunc $IntToStr" {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            *l = format!("{indent}l1 trunc \" 3\" $IntToStrEnh");
            changed = true;
        } else if let Some(letter) = letter_in_front(l) {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            *l = format!("{indent}(L.$.Matrix_NewNr) 3 $SetLengthR \"{letter}\" $+");
            changed = true;
        }
    }
    changed
}

/// Volvo Wright door scripts gate the rear close macro on the parking brake and on both rear door
/// leaves already being exactly `1`.  Their working outside-CL button bypasses those guards and
/// writes the close target directly.  Make the shared macro do the same while keeping the patch
/// specific to this known script shape.
fn volvo_rear_door_close(lines: &mut [String]) -> bool {
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        let text = line.trim();
        if text == "{macro:trg_bus_dooraftclose}" {
            start = Some(i + 1);
        } else if text == "{end}" {
            if let Some(begin) = start.take() {
                ranges.push((begin, i));
            }
        }
    }
    let mut changed = false;
    let mut recognised = false;
    for (start, end) in ranges {
        let body = &lines[start..end];
        let has = |needle: &str| body.iter().any(|line| line.trim() == needle);
        if !(has("(L.L.bremse_feststell_sw) 1 =")
            && has("(L.L.cockpit_button_smallhb) 1 = ||")
            && has("(L.L.door_2) 1 = &&")
            && has("(L.L.door_3) 1 = &&")
            && body.iter().any(|line| line.contains("S.L.doorTarget_23")))
        {
            continue;
        }
        recognised = true;
        for line in &mut lines[start..end] {
            if matches!(
                line.trim(),
                "(L.L.bremse_feststell_sw) 1 ="
                    | "(L.L.cockpit_button_smallhb) 1 = ||"
                    | "(L.L.door_2) 1 = &&"
                    | "(L.L.door_3) 1 = &&"
            ) {
                line.clear();
            }
        }
        lines[start] = "0 (S.L.bdoor_sound_played)".to_string();
        // The close animation emits ev_doortriggerclose_2 only when this latch is clear.
        // Some variants leave it set after the previous cycle, silencing the warning on the
        // next forced close.
        if start + 1 < end {
            lines[start + 1] = "1".to_string();
        }
        changed = true;
    }
    if recognised {
        // The forced-close button sets bdoor_embtn_cls while the leaves move.  The stock
        // warning-lamp condition unnecessarily excludes that state, so backdoor_buzzer never
        // reaches the model's mdoor_warn material on these variants.
        for i in 0..lines.len() {
            if lines[i].trim() != "1 (S.L.backdoor_buzzer)" {
                continue;
            }
            let begin = i.saturating_sub(12);
            if let Some(j) = (begin..i)
                .rev()
                .find(|&j| lines[j].trim() == "(L.L.bdoor_embtn_cls) 0 = &&")
            {
                lines[j].clear();
                changed = true;
            }
        }
    }
    changed
}

/// `"E" (L.$.Matrix_NewNr) 2 $SetLengthR " " $+ $+` or `" D" (L.$.Matrix_NewNr) 1
/// $SetLengthR " " $+ $+`: the letter written in front of the number.
fn letter_in_front(line: &str) -> Option<char> {
    let t = line.trim();
    let rest = t.strip_prefix('"')?;
    let (prefix, rest) = rest.split_once('"')?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() != 7
        || words[0] != "(L.$.Matrix_NewNr)"
        || !matches!(words[1], "1" | "2")
        || words[2] != "$SetLengthR"
        || words[3] != "\""
        || words[4] != "\""
        || words[5] != "$+"
        || words[6] != "$+"
    {
        return None;
    }
    let mut letters = prefix.chars().filter(|c| !c.is_whitespace());
    let letter = letters.next().filter(|c| c.is_ascii_alphabetic())?;
    letters.next().is_none().then_some(letter)
}

/// Aachen ibox digit entry (modes 2 / 7 / 10): stock script keeps digits in float
/// `ibox_eingabe` (`0` × 10 stays 0) and blanks `ibox_Zahleingabe` whenever that float is 0,
/// so a leading zero never appears. Hof routes are two-digit codes (`01`…`17`) looked up as
/// `line×100 + route` via `GetRouteIndex`. PIN compare still uses the float vs `ibox_PIN`.
///
/// Stock `ibox_tastatur` also enforces a hard **4-digit** float cap (`ibox_eingabe > 9999`
/// then `÷10`) — comment `'4 Ziffern Begrenzung`. That is not an openOMSI invention.
/// Hof trip `3301` (Vaals Flats) is **not** typed as one five-digit string `03301`: the
/// header pads line to three digits (`033`) and shows route (`01`). Enter **Linie** `33`
/// or `033`, then **Route** `01` → `GetRouteIndex(33×100+1)`.
///
/// Display rules after this patch (OMSI-authentic typing — no auto-pad):
/// - Mode 2 (PIN): same key-edge string buffer; each digit appends `"*"` (masked). Leading
///   zero adds a star even while the float stays 0. Cap at 4 stars (stock float width).
/// - Mode 7 (Linie) / Mode 10 (Route): `ibox_Zahleingabe` is a string digit buffer. Starts
///   empty — no prefilling `"00"`, no `"02" $IntToStrEnh`. Press `0` → `"0"`; `0` then `1`
///   → `"01"`; only `1` → `"1"`. At most 4 characters (same as stock `9999` cap).
///   Submit/lookup still uses float `ibox_eingabe`.
/// - Entering any of these modes clears the buffer so leftovers do not carry over.
///
/// `clickspots_reset` (called at the start of every `ibox_frame`) stores `""` into
/// `ibox_Zahleingabe`. Stock rewrote the field each frame with `$IntToStr` / star-mask, so
/// that wipe was harmless. A key-edge digit buffer must survive the reset or every digit
/// vanishes before the next frame.
fn aachen_ibox_digit_append_block(indent: &str, masked: bool) -> Vec<String> {
    let mut block = Vec::new();
    let ch0 = if masked { "*" } else { "0" };
    block.push(format!("{indent}(L.L.ibox_taste_D11) 1 ="));
    block.push(format!("{indent}{{if}}"));
    // Stock float rejects a 5th digit (`> 9999` → ÷10); keep the string buffer in sync.
    block.push(format!("{indent}\t(L.$.ibox_Zahleingabe) $length 4 <"));
    block.push(format!("{indent}\t{{if}}"));
    block.push(format!(
        "{indent}\t\t(L.$.ibox_Zahleingabe) \"{ch0}\" $+ (S.$.ibox_Zahleingabe)"
    ));
    block.push(format!("{indent}\t{{endif}}"));
    block.push(format!("{indent}{{endif}}"));
    for d in 1..=9 {
        let ch = if masked {
            "*".to_string()
        } else {
            d.to_string()
        };
        block.push(format!("{indent}(L.L.ibox_taste_D{d}) 1 ="));
        block.push(format!("{indent}{{if}}"));
        block.push(format!("{indent}\t(L.$.ibox_Zahleingabe) $length 4 <"));
        block.push(format!("{indent}\t{{if}}"));
        block.push(format!(
            "{indent}\t\t(L.$.ibox_Zahleingabe) \"{ch}\" $+ (S.$.ibox_Zahleingabe)"
        ));
        block.push(format!("{indent}\t{{endif}}"));
        block.push(format!("{indent}{{endif}}"));
    }
    block
}

/// Rewrite unguarded `Zahleingabe` append blocks from an older compat pass so each key
/// only appends while `$length 4 <` (stock `'4 Ziffern Begrenzung'`).
fn aachen_ibox_upgrade_digit_length_cap(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    let mut i = 0;
    while i + 3 < lines.len() {
        let n1 = norm(&lines[i]);
        let is_digit_key = n1.starts_with("(L.L.ibox_taste_D") && n1.ends_with(") 1 =");
        if !is_digit_key || lines[i + 1].trim() != "{if}" {
            i += 1;
            continue;
        }
        let n2 = norm(&lines[i + 2]);
        let append = n2.starts_with("(L.$.ibox_Zahleingabe) \"")
            && n2.ends_with("\" $+ (S.$.ibox_Zahleingabe)");
        if !append || lines[i + 3].trim() != "{endif}" {
            i += 1;
            continue;
        }
        // Already guarded (length check sits between `{if}` and the append).
        if n2.contains("$length") {
            i += 1;
            continue;
        }
        let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
        let append_line = lines[i + 2].clone();
        let append_body = append_line.trim_start();
        let block = [
            lines[i].clone(),
            format!("{indent}{{if}}"),
            format!("{indent}\t(L.$.ibox_Zahleingabe) $length 4 <"),
            format!("{indent}\t{{if}}"),
            format!("{indent}\t\t{append_body}"),
            format!("{indent}\t{{endif}}"),
            format!("{indent}{{endif}}"),
        ];
        lines.splice(i..=i + 3, block);
        changed = true;
        i += 7;
    }
    changed
}

fn aachen_ibox_route_pad(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| lines.iter().any(|l| norm(l) == s);
    if !has("(L.L.IBIS_LinieKurs) 100 * l4 + (M.V.GetRouteIndex) (S.L.ibox_routenindex)")
        || !has("\"\" (S.$.ibox_Zahleingabe)")
    {
        return false;
    }

    let blanking_done = has(
        "(L.L.ibox_eingabe) 0 = (L.L.ibox_modus) 2 = ! && (L.L.ibox_modus) 7 = ! && (L.L.ibox_modus) 10 = ! &&",
    );
    let length_cap = has("(L.$.ibox_Zahleingabe) $length 4 <");
    let pin_buf_done = has("(L.$.ibox_Zahleingabe) \"*\" $+ (S.$.ibox_Zahleingabe)");
    let digit_buf_done = has("(L.$.ibox_Zahleingabe) \"0\" $+ (S.$.ibox_Zahleingabe)");
    let clickspots_stripped = lines.iter().any(|l| {
        let n = norm(l);
        n.contains("(S.$.ibox_StatuszeileMitte)")
            && n.contains("(S.$.ibox_StatuszeileRechts)")
            && !n.contains("(S.$.ibox_Zahleingabe)")
    });
    let clickspots_modes_done = has("(L.L.ibox_modus) 2 = !")
        && lines.iter().any(|l| {
            let n = norm(l);
            n == "(L.L.ibox_modus) 7 = ! &&" || n == "(L.L.ibox_modus) 7 = !"
        });
    let backspace_multi = has("(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)")
        && has("(L.L.ibox_modus) 7 = ||");
    // Fully applied already (incl. stock-matching 4-char string cap).
    if digit_buf_done
        && pin_buf_done
        && length_cap
        && blanking_done
        && clickspots_stripped
        && clickspots_modes_done
        && backspace_multi
    {
        return false;
    }
    // Stock (or older partial patch) must still expose a Zahleingabe IntToStr / PIN mask site,
    // or an older digit buffer that still lacks the 4-char length guard.
    let stock_display = has("(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)")
        || has("(L.L.ibox_eingabe) $IntToStr $length s0");
    let needs_length_cap_upgrade = (digit_buf_done || pin_buf_done) && !length_cap;
    if !digit_buf_done && !pin_buf_done && !stock_display && !needs_length_cap_upgrade {
        return false;
    }
    let mut changed = false;

    // Older compat append lacked `$length 4 <` — rewrite those key blocks in place.
    if needs_length_cap_upgrade {
        changed |= aachen_ibox_upgrade_digit_length_cap(lines);
    }

    // Mode 2 (PIN): replace float-length star mask with key-edge "*" append (leading 0 = extra *).
    if !pin_buf_done {
        changed |= aachen_ibox_pin_buffer(lines);
    }

    // Mode 7 (Linie): digit buffer. Guard with "still mode 7" so Enter→route in the same
    // block cannot append after the field was cleared for mode 10.
    // Also replaces the older compat shape that kept guarded `$IntToStr`.
    changed |= aachen_ibox_line_buffer(lines);

    // Mode 10 (Route): typed digits append to Zahleingabe (preserves leading zeros).
    changed |= aachen_ibox_route_buffer(lines);

    // Backspace (D10): trim the string buffer in PIN / line / route entry.
    if !backspace_multi {
        changed |= aachen_ibox_backspace(lines);
    }

    // Skip blanking while PIN / line / route entry holds a typed leading zero (float still 0).
    let blanking_target =
        "(L.L.ibox_eingabe) 0 = (L.L.ibox_modus) 2 = ! && (L.L.ibox_modus) 7 = ! && (L.L.ibox_modus) 10 = ! &&";
    if !blanking_done {
        for i in 0..lines.len() {
            let n = norm(&lines[i]);
            // Stock or older "mode 10 only" guard.
            if n != "(L.L.ibox_eingabe) 0 ="
                && n != "(L.L.ibox_eingabe) 0 = (L.L.ibox_modus) 10 = ! &&"
            {
                continue;
            }
            let Some(j) = lines[i + 1..].iter().position(|l| !l.trim().is_empty()) else {
                continue;
            };
            let j = i + 1 + j;
            if lines[j].trim() != "{if}" {
                continue;
            }
            let Some(k) = lines[j + 1..].iter().position(|l| !l.trim().is_empty()) else {
                continue;
            };
            let k = j + 1 + k;
            if norm(&lines[k]) != "\"\" (S.$.ibox_Zahleingabe)" {
                continue;
            }
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!("{indent}{blanking_target}");
            changed = true;
            break;
        }
    }

    // Clear the digit field when entering PIN / line / route entry.
    for i in 0..lines.len() {
        let n = norm(&lines[i]);
        if n == "10 (S.L.ibox_modus) 0 (S.L.ibox_eingabe)" {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!(
                "{indent}10 (S.L.ibox_modus) 0 (S.L.ibox_eingabe) \"\" (S.$.ibox_Zahleingabe)"
            );
            changed = true;
        }
    }
    let mut i = 0;
    while i + 1 < lines.len() {
        let a = norm(&lines[i]);
        let b = norm(&lines[i + 1]);
        let clears_eingabe =
            b == "0 (S.L.ibox_eingabe)" || b == "0 (S.L.ibox_taste_D12) (S.L.ibox_eingabe)";
        let entry_mode = a == "7 (S.L.ibox_modus)" || a == "2 (S.L.ibox_modus)";
        if entry_mode && clears_eingabe {
            let next = lines.get(i + 2).map(|l| norm(l)).unwrap_or_default();
            if next != "\"\" (S.$.ibox_Zahleingabe)" {
                let indent: String =
                    lines[i + 1].chars().take_while(|c| c.is_whitespace()).collect();
                lines.insert(i + 2, format!("{indent}\"\" (S.$.ibox_Zahleingabe)"));
                changed = true;
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    // Wrong PIN clears the float — clear the star buffer too.
    for i in 0..lines.len() {
        let n = norm(&lines[i]);
        if n == "(L.L.ibox_eingabe) (L.L.ibox_PIN) = ! {if} 0 (S.L.ibox_eingabe) {endif}" {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!(
                "{indent}(L.L.ibox_eingabe) (L.L.ibox_PIN) = ! {{if}} 0 (S.L.ibox_eingabe) \"\" (S.$.ibox_Zahleingabe) {{endif}}"
            );
            changed = true;
        }
    }

    // Keep digit buffers across frames in modes 2 / 7 / 10.
    changed |= aachen_ibox_click_spots(lines, clickspots_stripped, clickspots_modes_done);

    changed
}

/// `aachen_ibox_route_pad`'s mode 2 (PIN): the star mask built from the typed digits.
fn aachen_ibox_pin_buffer(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    let mut in_mode2 = false;
    let mut i = 0;
    while i + 1 < lines.len() {
        let n = norm(&lines[i]);
        if n == "(L.L.ibox_modus) 2 =" {
            in_mode2 = true;
            i += 1;
            continue;
        }
        if in_mode2 && (n.starts_with("(L.L.ibox_modus)") || n == "{end}") {
            in_mode2 = false;
        }
        if in_mode2
            && n == "(L.L.ibox_eingabe) $IntToStr $length s0"
            && norm(&lines[i + 1]) == "\"*\" l0 $* (S.$.ibox_Zahleingabe)"
        {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let block = aachen_ibox_digit_append_block(&indent, true);
            let n_lines = block.len();
            lines.splice(i..=i + 1, block);
            changed = true;
            in_mode2 = false;
            i += n_lines;
            continue;
        }
        i += 1;
    }
    changed
}

/// `aachen_ibox_route_pad`'s mode 7 (line): the typed digits appended to the buffer, also over
/// the older patch's guarded `$IntToStr`.
fn aachen_ibox_line_buffer(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let n = norm(&lines[i]);
        // Older patch: guarded IntToStr — replace the whole 6-line shape.
        if n == "(L.L.ibox_modus) 7 ="
            && lines.get(i + 1).is_some_and(|l| l.trim() == "{if}")
            && lines
                .get(i + 2)
                .is_some_and(|l| norm(l) == "(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)")
            && lines.get(i + 3).is_some_and(|l| l.trim() == "{else}")
            && lines
                .get(i + 4)
                .is_some_and(|l| norm(l) == "\"\" (S.$.ibox_Zahleingabe)")
            && lines.get(i + 5).is_some_and(|l| l.trim() == "{endif}")
        {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let mut block = vec![
                format!("{indent}(L.L.ibox_modus) 7 ="),
                format!("{indent}{{if}}"),
            ];
            block.extend(aachen_ibox_digit_append_block(&format!("{indent}\t"), false));
            block.push(format!("{indent}{{endif}}"));
            let n_lines = block.len();
            lines.splice(i..=i + 5, block);
            changed = true;
            i += n_lines;
            continue;
        }
        i += 1;
    }

    let mut in_mode7 = false;
    let mut i = 0;
    while i < lines.len() {
        let n = norm(&lines[i]);
        if n == "(L.L.ibox_modus) 7 =" {
            in_mode7 = true;
            i += 1;
            continue;
        }
        if in_mode7 && (n == "{end}" || (n.starts_with("(L.L.ibox_modus)") && n != "(L.L.ibox_modus) 7 ="))
        {
            in_mode7 = false;
        }
        if in_mode7 && n == "(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)" {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let mut block = vec![
                format!("{indent}(L.L.ibox_modus) 7 ="),
                format!("{indent}{{if}}"),
            ];
            block.extend(aachen_ibox_digit_append_block(&format!("{indent}\t"), false));
            block.push(format!("{indent}{{endif}}"));
            let n_lines = block.len();
            lines.splice(i..=i, block);
            changed = true;
            in_mode7 = false;
            i += n_lines;
            continue;
        }
        i += 1;
    }
    changed
}

/// `aachen_ibox_route_pad`'s mode 10 (route): the typed digits appended to the buffer.
fn aachen_ibox_route_buffer(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    let mut in_mode10 = false;
    let mut i = 0;
    while i < lines.len() {
        let n = norm(&lines[i]);
        if n == "(L.L.ibox_modus) 10 =" {
            in_mode10 = true;
            i += 1;
            continue;
        }
        if in_mode10 && (n.starts_with("(L.L.ibox_modus)") || n == "{end}") {
            in_mode10 = false;
        }
        if in_mode10 && n == "(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)" {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let block = aachen_ibox_digit_append_block(&indent, false);
            let n_lines = block.len();
            lines.splice(i..=i, block);
            changed = true;
            in_mode10 = false;
            i += n_lines;
            continue;
        }
        i += 1;
    }
    changed
}

/// `aachen_ibox_route_pad`'s backspace (D10): the buffer trimmed in PIN, line and route entry.
fn aachen_ibox_backspace(lines: &mut Vec<String>) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    let mut i = 0;
    while i + 3 < lines.len() {
        // Older patch only guarded mode 10 — widen the condition.
        if norm(&lines[i]) == "(L.L.ibox_modus) 10 ="
            && lines.get(i + 1).is_some_and(|l| l.trim() == "{if}")
            && lines.get(i + 2).is_some_and(|l| {
                norm(l) == "(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)"
            })
            && lines.get(i + 3).is_some_and(|l| l.trim() == "{endif}")
            && i >= 1
            && norm(&lines[i - 1]) == "(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)"
        {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let trim = [
                format!("{indent}(L.L.ibox_modus) 2 ="),
                format!("{indent}(L.L.ibox_modus) 7 = ||"),
                format!("{indent}(L.L.ibox_modus) 10 = ||"),
                format!("{indent}{{if}}"),
                format!("{indent}\t(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)"),
                format!("{indent}{{endif}}"),
            ];
            lines.splice(i..=i + 3, trim);
            changed = true;
            break;
        }
        if norm(&lines[i]) == "(L.L.ibox_taste_D10) 1 ="
            && lines.get(i + 1).is_some_and(|l| l.trim() == "{if}")
            && norm(&lines[i + 2]) == "(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)"
            && norm(&lines[i + 3]) == "0 (S.L.ibox_taste_D10)"
        {
            let indent: String = lines[i + 2].chars().take_while(|c| c.is_whitespace()).collect();
            let trim = [
                format!("{indent}(L.L.ibox_modus) 2 ="),
                format!("{indent}(L.L.ibox_modus) 7 = ||"),
                format!("{indent}(L.L.ibox_modus) 10 = ||"),
                format!("{indent}{{if}}"),
                format!("{indent}\t(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)"),
                format!("{indent}{{endif}}"),
            ];
            lines.splice(i + 3..i + 3, trim);
            changed = true;
            break;
        }
        i += 1;
    }
    changed
}

/// `aachen_ibox_route_pad`'s click spots: the buffer kept across frames in modes 2, 7 and 10
/// (the older patch guarded mode 10 only).
fn aachen_ibox_click_spots(lines: &mut Vec<String>, clickspots_stripped: bool, clickspots_modes_done: bool) -> bool {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut changed = false;
    if !clickspots_stripped {
        for i in 0..lines.len() {
            let n = norm(&lines[i]);
            if !(n.contains("(S.$.ibox_StatuszeileMitte)")
                && n.contains("(S.$.ibox_StatuszeileRechts)")
                && n.contains("(S.$.ibox_Zahleingabe)"))
            {
                continue;
            }
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let mut parts: Vec<&str> = lines[i].split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|p| *p == "(S.$.ibox_Zahleingabe)") {
                parts.remove(pos);
            }
            lines[i] = format!("{indent}{}", parts.join(" "));
            let guard = [
                format!("{indent}(L.L.ibox_modus) 2 = !"),
                format!("{indent}(L.L.ibox_modus) 7 = ! &&"),
                format!("{indent}(L.L.ibox_modus) 10 = ! &&"),
                format!("{indent}{{if}}"),
                format!("{indent}\t\"\" (S.$.ibox_Zahleingabe)"),
                format!("{indent}{{endif}}"),
            ];
            lines.splice(i + 1..i + 1, guard);
            changed = true;
            break;
        }
    } else if !clickspots_modes_done {
        // Older patch: only mode 10 was guarded — widen after the stripped Statuszeile line.
        for i in 0..lines.len() {
            let n = norm(&lines[i]);
            if !(n.contains("(S.$.ibox_StatuszeileMitte)")
                && n.contains("(S.$.ibox_StatuszeileRechts)")
                && !n.contains("(S.$.ibox_Zahleingabe)"))
            {
                continue;
            }
            if norm(&lines.get(i + 1).cloned().unwrap_or_default()) == "(L.L.ibox_modus) 10 = !"
                && lines.get(i + 2).is_some_and(|l| l.trim() == "{if}")
            {
                let indent: String =
                    lines[i + 1].chars().take_while(|c| c.is_whitespace()).collect();
                let guard = [
                    format!("{indent}(L.L.ibox_modus) 2 = !"),
                    format!("{indent}(L.L.ibox_modus) 7 = ! &&"),
                    format!("{indent}(L.L.ibox_modus) 10 = ! &&"),
                    format!("{indent}{{if}}"),
                    format!("{indent}\t\"\" (S.$.ibox_Zahleingabe)"),
                    format!("{indent}{{endif}}"),
                ];
                // Replace old 10-only guard (4 lines).
                lines.splice(i + 1..=i + 4, guard);
                changed = true;
            }
            break;
        }
    }
    changed
}

/// Apply every fix that recognises `lines`; returns the names of those applied.
pub fn patch(lines: &mut Vec<String>) -> Vec<&'static str> {
    let mut applied = Vec::new();
    if four_char_matrix(lines) {
        applied.push("4-character line matrix: number padded to three digits");
    }
    if volvo_rear_door_close(lines) {
        applied.push("Volvo rear door closes while the leaves are moving");
    }
    if aachen_ibox_route_pad(lines) {
        applied.push("Aachen ibox: digit buffer for PIN/line/route; typed leading zero; max 4 chars");
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liaz_matrix_pads_the_number() {
        let src = "\t\t\t\t\t\tl1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n4 $SetLengthL\n\t(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert_eq!(patch(&mut lines).len(), 1);
        assert_eq!(lines[0], "\t\t\t\t\t\tl1 trunc \" 3\" $IntToStrEnh");
        // the stock 3-character matrix is left alone
        let mut stock: Vec<String> = "l1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n2 $SetLengthL\n\"E\" (L.$.Matrix_NewNr) 2 $SetLengthR $+".lines().map(String::from).collect();
        assert!(patch(&mut stock).is_empty());
    }

    #[test]
    fn liaz_matrix_letter_behind_the_digits() {
        let src = "l1 trunc $IntToStr\n(L.$.Matrix_NewNr) $length 1 <=\n4 $SetLengthL\n(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+\n\t\t\" D\" (L.$.Matrix_NewNr) 1 $SetLengthR \" \" $+ $+\n\"E\" (L.$.Matrix_NewNr) 2 $SetLengthR \" \" $+ $+\n\"BVG \"";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        patch(&mut lines);
        assert_eq!(lines[4], "\t\t(L.$.Matrix_NewNr) 3 $SetLengthR \"D\" $+");
        assert_eq!(lines[5], "(L.$.Matrix_NewNr) 3 $SetLengthR \"E\" $+");
        assert_eq!(lines[6], "\"BVG \"");
    }

    #[test]
    fn volvo_rear_door_close_does_not_wait_for_both_leaves() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.bremse_feststell_sw) 1 =\n(L.L.cockpit_button_smallhb) 1 = ||\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{if}\n0 (S.L.doorTarget_23)\n{endif}\n{end}\n{macro:other}\n(L.L.door_2) 1 = &&";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let applied = patch(&mut lines);
        assert!(applied.iter().any(|name| name.starts_with("Volvo rear door")));
        assert_eq!(lines[1], "0 (S.L.bdoor_sound_played)");
        assert_eq!(lines[2], "1");
        assert!(lines[3..5].iter().all(String::is_empty));
        assert_eq!(lines[10], "(L.L.door_2) 1 = &&");
    }

    #[test]
    fn volvo_rear_door_fix_requires_the_known_guard_shape() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{end}";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert!(patch(&mut lines).is_empty());
        assert_eq!(lines[1], "(L.L.door_2) 1 = &&");
    }

    #[test]
    fn volvo_rear_door_warning_allows_forced_close_state() {
        let src = "{macro:trg_bus_dooraftclose}\n(L.L.bremse_feststell_sw) 1 =\n(L.L.cockpit_button_smallhb) 1 = ||\n(L.L.door_2) 1 = &&\n(L.L.door_3) 1 = &&\n{if}\n0 (S.L.doorTarget_23)\n{endif}\n{end}\n(L.L.door_2) 0.1 >\n(L.L.doorTarget_23) 0 = &&\n(L.L.bdoor_embtn_cls) 0 = &&\n(L.L.doorbuzzer_timer) 0 >\n(L.L.doorbuzzer_timer) 1.3 < && ||\n{if}\n1 (S.L.backdoor_buzzer)\n{endif}";
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        assert_eq!(patch(&mut lines).len(), 1);
        assert!(!lines.iter().any(|line| line.trim() == "(L.L.bdoor_embtn_cls) 0 = &&"));
    }

    #[test]
    fn aachen_ibox_route_digit_buffer_preserves_typed_leading_zero() {
        let src = r#"{macro:clickspots_reset}
"" (S.$.ibox_StatuszeileMitte) (S.$.ibox_StatuszeileRechts) (S.$.ibox_Zahleingabe) (S.$.ibox_delay) (S.$.ibox_hstverlauf)
{end}
{macro:ibox_tastatur}
(L.L.ibox_taste_D10) 1 =
{if}
	(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)
	0 (S.L.ibox_taste_D10)
{endif}
{end}
(L.L.ibox_modus) 2 =
{if}
	(L.L.ibox_eingabe) $IntToStr $length s0
	"*" l0 $* (S.$.ibox_Zahleingabe)
	(L.L.ibox_eingabe) (L.L.ibox_PIN) = ! {if} 0 (S.L.ibox_eingabe) {endif}
{endif}
(L.L.ibox_modus) 7 =
{if}
	(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)
{endif}
(L.L.ibox_modus) 10 =
{if}
	(L.L.ibox_eingabe) s4 (S.L.ibox_Route)
	(L.L.IBIS_LinieKurs) 100 * l4 + (M.V.GetRouteIndex) (S.L.ibox_routenindex)
	(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)
{endif}
(L.L.ibox_modus) 11 =
{if}
	(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)
{endif}
(L.L.ibox_eingabe) 0 =
{if}
	"" (S.$.ibox_Zahleingabe)
{endif}
2 (S.L.ibox_modus)
0 (S.L.ibox_eingabe)
7 (S.L.ibox_modus)
0 (S.L.ibox_eingabe)
10 (S.L.ibox_modus) 0 (S.L.ibox_eingabe)
"#;
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let applied = patch(&mut lines);
        assert!(applied.iter().any(|n| n.contains("Aachen ibox")));
        let n = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
        // Digit / star buffer append — no IntToStrEnh auto-pad; stock 4-char cap.
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) $length 4 <"));
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) \"0\" $+ (S.$.ibox_Zahleingabe)"));
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) \"1\" $+ (S.$.ibox_Zahleingabe)"));
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) \"*\" $+ (S.$.ibox_Zahleingabe)"));
        assert!(!lines.iter().any(|l| l.contains("IntToStrEnh") && l.contains("ibox_Zahleingabe")));
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)"));
        // Mode 11 stays plain IntToStr; PIN mask / line IntToStr are gone.
        assert!(lines.iter().any(|l| n(l) == "(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)"));
        assert!(!lines.iter().any(|l| n(l) == "(L.L.ibox_eingabe) $IntToStr $length s0"));
        assert!(lines.iter().any(|l| {
            n(l) == "(L.L.ibox_eingabe) 0 = (L.L.ibox_modus) 2 = ! && (L.L.ibox_modus) 7 = ! && (L.L.ibox_modus) 10 = ! &&"
        }));
        assert!(lines.iter().any(|l| {
            n(l) == "10 (S.L.ibox_modus) 0 (S.L.ibox_eingabe) \"\" (S.$.ibox_Zahleingabe)"
        }));
        // clickspots_reset must not wipe buffers in PIN / line / route entry.
        assert!(lines.iter().any(|l| {
            let x = n(l);
            x.contains("(S.$.ibox_StatuszeileMitte)")
                && x.contains("(S.$.ibox_StatuszeileRechts)")
                && !x.contains("(S.$.ibox_Zahleingabe)")
        }));
        assert!(lines.iter().any(|l| n(l) == "(L.L.ibox_modus) 2 = !"));
        assert!(lines.iter().any(|l| n(l) == "(L.L.ibox_modus) 7 = ! &&"));
        assert!(lines.iter().any(|l| n(l) == "(L.L.ibox_modus) 10 = ! &&"));
        // Idempotent.
        assert!(patch(&mut lines).is_empty());
    }

    #[test]
    fn aachen_ibox_upgrades_unguarded_digit_buffer_to_4_char_cap() {
        // Prior compat append without `$length 4 <` must be rewritten in place.
        let src = r#"{macro:clickspots_reset}
"" (S.$.ibox_StatuszeileMitte) (S.$.ibox_StatuszeileRechts) (S.$.ibox_delay) (S.$.ibox_hstverlauf)
(L.L.ibox_modus) 2 = !
(L.L.ibox_modus) 7 = ! &&
(L.L.ibox_modus) 10 = ! &&
{if}
	"" (S.$.ibox_Zahleingabe)
{endif}
{end}
{macro:ibox_tastatur}
(L.L.ibox_taste_D10) 1 =
{if}
	(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)
	0 (S.L.ibox_taste_D10)
	(L.L.ibox_modus) 7 = ||
	(L.L.ibox_modus) 2 = ||
	(L.L.ibox_modus) 10 = ||
	{if}
		(L.$.ibox_Zahleingabe) 1 $cutEnd (S.$.ibox_Zahleingabe)
	{endif}
{endif}
{end}
(L.L.ibox_modus) 2 =
{if}
	(L.L.ibox_taste_D11) 1 =
	{if}
		(L.$.ibox_Zahleingabe) "*" $+ (S.$.ibox_Zahleingabe)
	{endif}
{endif}
(L.L.ibox_modus) 7 =
{if}
	(L.L.ibox_taste_D11) 1 =
	{if}
		(L.$.ibox_Zahleingabe) "0" $+ (S.$.ibox_Zahleingabe)
	{endif}
{endif}
(L.L.ibox_modus) 10 =
{if}
	(L.L.ibox_eingabe) s4 (S.L.ibox_Route)
	(L.L.IBIS_LinieKurs) 100 * l4 + (M.V.GetRouteIndex) (S.L.ibox_routenindex)
	(L.L.ibox_taste_D1) 1 =
	{if}
		(L.$.ibox_Zahleingabe) "1" $+ (S.$.ibox_Zahleingabe)
	{endif}
{endif}
(L.L.ibox_eingabe) 0 = (L.L.ibox_modus) 2 = ! && (L.L.ibox_modus) 7 = ! && (L.L.ibox_modus) 10 = ! &&
{if}
	"" (S.$.ibox_Zahleingabe)
{endif}
"#;
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let applied = patch(&mut lines);
        assert!(applied.iter().any(|n| n.contains("Aachen ibox")), "{applied:?}");
        let n = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(lines.iter().any(|l| n(l) == "(L.$.ibox_Zahleingabe) $length 4 <"));
        assert!(patch(&mut lines).is_empty(), "idempotent after length-cap upgrade");
    }
}
