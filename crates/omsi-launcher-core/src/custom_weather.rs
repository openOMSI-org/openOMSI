use crate::{content_path, Result};
use anyhow::{anyhow, Context};
pub use omsi_content::weather_choice::CustomWeather;

pub fn custom_weather(choice: &str) -> Result<CustomWeather> {
    let choice = choice.trim();
    if let Some(c) = CustomWeather::parse(choice) {
        return Ok(c);
    }
    if !choice.to_ascii_lowercase().ends_with(".owt") {
        return Err(anyhow!("{choice} is no weather file"));
    }
    let w = omsi_content::weather::Weather::load(&content_path(choice)?).with_context(|| format!("reading {choice}"))?;
    Ok(CustomWeather::from_weather(&w, 1.0, (w.ground_wet[0] / 255.0).clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{duty_args_for_root, Duty};
    use std::path::Path;

    #[test]
    fn a_custom_weather_reaches_the_game_as_it_was_set() {
        let set = CustomWeather { visibility_m: 800.0, wind_speed: 12.5, temp_c: -5.0, precip: 2, precip_intensity: 200.0, road_wetness: 0.4, snow_cover: true, snow_on_road: true, ..Default::default() };
        let d = Duty { map: "maps/x/global.cfg".into(), bus: "Vehicles/x.bus".into(), time: "09:00".into(), weather: Some(set.encode()), ..Default::default() };
        let a = duty_args_for_root(&d, Path::new("test-omsi")).unwrap();
        let k = a.iter().position(|x| x == "--weather").unwrap();
        assert_eq!(CustomWeather::parse(&a[k + 1]), Some(set.clone()));
        assert_eq!(custom_weather(&a[k + 1]).unwrap(), set);
        assert_eq!(custom_weather("custom:").unwrap(), CustomWeather::default());
        assert!(custom_weather("cycle").is_err());
    }
}
