use super::*;

#[test]
fn draws_mostly_hold_together() {
    let mut ok = 0;
    let mut why = std::collections::BTreeMap::new();
    let mut kinds = std::collections::BTreeMap::new();
    for k in 0..200u64 {
        let mut rng = Rng::new(k * 7919);
        match Layout::generate(&mut rng) {
            Ok(l) => {
                ok += 1;
                *kinds
                    .entry((
                        l.mines.len(),
                        l.washes.len(),
                        l.buttes.len(),
                        l.size as i32 / 10 * 10,
                    ))
                    .or_insert(0) += 1;
            }
            Err(line) => *why.entry(line).or_insert(0) += 1,
        }
    }
    eprintln!("{ok} of 200 hold together; failed at {why:?}");
    eprintln!("(mines, washes, buttes, size): {kinds:?}");
    assert!(ok > 40, "{ok} of 200");
}
