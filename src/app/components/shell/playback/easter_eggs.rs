//! Easter egg animations for the playback bar.

use gtk::glib;

pub const EGG_CHANCE: i32 = 250; // 1 out 250 chance, on average once an hour

#[derive(Clone, Copy, Debug)]
pub enum EasterEgg {
    Fish,
    Notes,
    TRex,
    Snail,
}

impl EasterEgg {
    const ALL: [Self; 4] = [Self::Fish, Self::Notes, Self::TRex, Self::Snail];

    pub fn random() -> Self {
        Self::ALL[glib::random_int_range(0, Self::ALL.len() as i32) as usize]
    }

    pub fn frame_ms(self) -> u64 {
        match self {
            Self::Fish => 120,
            Self::Notes => 300,
            Self::TRex => 130,
            Self::Snail => 450,
        }
    }

    pub fn frames(self) -> Vec<String> {
        let frames = match self {
            Self::Fish => fish(),
            Self::Notes => notes(),
            Self::TRex => t_rex(),
            Self::Snail => snail(),
        };
        frames
            .into_iter()
            .map(|frame| format!("<span font_family=\"monospace\">{frame}</span>"))
            .collect()
    }
}

fn gap(cells: usize) -> String {
    " ".repeat(cells)
}

fn raised(text: &str, up: bool) -> String {
    if up {
        format!("<span rise=\"6pt\">{text}</span>")
    } else {
        text.to_string()
    }
}

fn fish() -> Vec<String> {
    const WINDOW: usize = 32;
    let school = ["<°)))><"; 3].join(&gap(3));
    let len = school.chars().count() + 2;
    (0..=WINDOW + len)
        .map(|k| {
            let bubble = if k % 4 < 2 { "°" } else { "o" };
            let line: Vec<char> = format!("{}{school} {bubble}{}", gap(WINDOW), gap(WINDOW))
                .chars()
                .collect();
            let frame: String = line[k..k + WINDOW].iter().collect();
            glib::markup_escape_text(&frame).to_string()
        })
        .collect()
}

fn notes() -> Vec<String> {
    (0..16)
        .map(|k| {
            ["♪", "♫", "♬", "♪"]
                .iter()
                .enumerate()
                .map(|(i, note)| raised(note, (k + i) % 2 == 0))
                .collect::<Vec<_>>()
                .join(&gap(3))
        })
        .collect()
}

fn t_rex() -> Vec<String> {
    const TRACK: usize = 20;
    const DINO: usize = 14;
    const JUMP: &str = "11pt";
    let headroom = format!("<span rise=\"{JUMP}\"> </span>");
    let pass = (0..=TRACK - 2).filter(|&c| c != DINO - 1).map(move |c| {
        let dino = if c == DINO - 2 || c == DINO {
            format!("<span rise=\"{JUMP}\">🦖</span>")
        } else {
            "🦖".to_string()
        };
        if c < DINO {
            format!(
                "{}🌵{}{dino}{}{headroom}",
                gap(c),
                gap(DINO - c - 2),
                gap(TRACK - DINO)
            )
        } else {
            format!(
                "{}{dino}{}🌵{}{headroom}",
                gap(DINO),
                gap(c - DINO),
                gap(TRACK - c - 2)
            )
        }
    });
    pass.clone().chain(pass).collect()
}

fn snail() -> Vec<String> {
    const TRACK: usize = 18;
    (0..=TRACK)
        .rev()
        .map(|i| {
            let trail = "·".repeat(TRACK - i);
            format!("{}🐌<span alpha=\"40%\">{trail}</span>", gap(i))
        })
        .collect()
}
