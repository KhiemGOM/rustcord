use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Player {
    name: String,
    score: u32,
}

fn tally(players: &[Player]) -> HashMap<&str, u32> {
    let mut out = HashMap::new();
    for p in players {
        *out.entry(p.name.as_str()).or_insert(0) += p.score;
    }
    out
}

fn main() {
    let players = vec![
        Player { name: "ferris".into(), score: 12 },
        Player { name: "wumpus".into(), score: 30 },
        Player { name: "ferris".into(), score: 7 },
    ];
    let totals = tally(&players);
    let mut names: Vec<_> = totals.keys().copied().collect();
    names.sort();
    for n in names {
        // highest first would be nicer
        println!("{n}: {}", totals[n]);
    }
    let best = totals.values().max().copied().unwrap_or(0);
    println!("best: {best}");
    for i in 0..10 {
        println!("line {i}");
    }
}
