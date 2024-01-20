use libccanvas::{
    bindings::{Colour, Discriminator, EventVariant, Subscription},
    client::{Client, ClientConfig, LifetimeSuppressor},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Rect {
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn min() -> Self {
        Self {
            width: 0,
            height: 0,
        }
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new(ClientConfig::default()).await;

    let (min, (mut term_width, mut term_height), _, _) = tokio::join!(
        client.get(
            "!ccanvas-saver-dimensions".to_string(),
            Discriminator::master(),
        ),
        client.term_size(),
        client.watch(
            "!ccanvas-saver-dimensions".to_string(),
            Discriminator::master()
        ),
        client.subscribe(Subscription::ScreenResize.with_priority(10))
    );

    let mut min = min
        .map(|value| serde_json::from_value(value).unwrap_or(Rect::min()))
        .unwrap_or(Rect::min());
    let mut lifetime_suppressor = None;

    update(
        &client,
        term_width,
        term_height,
        &mut lifetime_suppressor,
        &min,
    )
    .await;

    loop {
        let event = client.recv().await;

        match event.get() {
            EventVariant::Resize { width, height } => {
                term_width = *width;
                term_height = *height;
                update(
                    &client,
                    term_width,
                    term_height,
                    &mut lifetime_suppressor,
                    &min,
                )
                .await;

                if lifetime_suppressor.is_some() {
                    event.done(false);
                }
            }
            EventVariant::ValueUpdated { label, new, .. }
                if label == "!ccanvas-saver-dimensions" =>
            {
                min = if let Ok(rect) = serde_json::from_value(new.clone()) {
                    rect
                } else {
                    continue;
                }
            }
            EventVariant::ValueRemoved { label, .. } if label == "!ccanvas-saver-dimensions" => {
                min = Rect::min()
            }
            _ => {}
        }
    }
}

async fn update(
    client: &Client,
    term_width: u32,
    term_height: u32,
    lifetime_suppressor: &mut Option<LifetimeSuppressor>,
    min: &Rect,
) {
    if term_width < min.width || term_height < min.height {
        client.clear_all();

        let lines = vec![
            "Terminal size too small:".to_string(),
            format!("Width = {term_width} Height = {term_height}"),
            "Needed for current config:".to_string(),
            format!("Width = {} Height = {}", min.width, min.height),
        ];

        if term_height >= 5 && lines.iter().all(|line| term_width >= line.len() as u32) {
            let y_offset = (term_height.saturating_sub(6)) / 2;

            // line 1
            ((term_width - lines[0].len() as u32) / 2..)
                .zip(lines[0].chars())
                .for_each(|(x, c)| client.setchar(x, y_offset, c));

            // line 2
            {
                let mut x = (term_width - lines[1].len() as u32) / 2;
                let width_colour = if term_width < min.width {
                    Colour::LightRed
                } else {
                    Colour::LightGreen
                };
                let height_colour = if term_height < min.height {
                    Colour::LightRed
                } else {
                    Colour::LightGreen
                };

                "Width = ".chars().for_each(|c| {
                    client.setchar(x, y_offset + 1, c);
                    x += 1;
                });
                term_width.to_string().chars().for_each(|c| {
                    client.setcharcoloured(x, y_offset + 1, c, width_colour, Colour::Reset);
                    x += 1;
                });
                " Height = ".chars().for_each(|c| {
                    client.setchar(x, y_offset + 1, c);
                    x += 1;
                });
                term_height.to_string().chars().for_each(|c| {
                    client.setcharcoloured(x, y_offset + 1, c, height_colour, Colour::Reset);
                    x += 1;
                });
            }

            // line 3 - 4
            lines.iter().skip(2).enumerate().for_each(|(index, line)| {
                ((term_width - line.len() as u32) / 2..)
                    .zip(line.chars())
                    .for_each(|(x, c)| client.setchar(x, y_offset + index as u32 + 3, c));
            })
        } else if term_height > 0 && term_width >= 9 {
            ((term_width - 9) / 2..)
                .zip("Too small".chars())
                .for_each(|(x, c)| client.setchar(x, (term_height - 1) / 2, c));
        }

        tokio::join!(
            async {
                if lifetime_suppressor.is_none() {
                    (*lifetime_suppressor, _) = tokio::join!(
                        client.suppress(Subscription::Everything, 10, Discriminator::master()),
                        client.set(
                            "!ccanvas-saver-ison".to_string(),
                            Discriminator::master(),
                            serde_json::to_value(true).unwrap()
                        )
                    );
                }
            },
            client.renderall(),
        );
    } else if lifetime_suppressor.is_some() {
        tokio::join!(
            client.unsuppress(std::mem::take(lifetime_suppressor).unwrap()),
            client.set(
                "!ccanvas-saver-ison".to_string(),
                Discriminator::master(),
                serde_json::to_value(false).unwrap()
            )
        );
    }
}
