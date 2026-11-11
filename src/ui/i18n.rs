use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Lang {
    #[default]
    En,
    Ru,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub(crate) fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub(crate) fn get() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::Ru as u8 { Lang::Ru } else { Lang::En }
}

pub(crate) fn tr(en: &'static str, ru: &'static str) -> &'static str {
    if get() == Lang::Ru { ru } else { en }
}

/// Display text of a string owned by a plugin/registry: translated when known, English otherwise.
pub(crate) fn shown<'a>(en: &'a str) -> &'a str {
    if get() == Lang::Ru { ru(en).unwrap_or(en) } else { en }
}

pub(crate) fn label(en: &'static str) -> &'static str {
    shown(en)
}

pub(crate) fn ru(en: &str) -> Option<&'static str> {
    let ru = match en {
        // panels, tools, spawners
        "Global Settings" => "Общие настройки",
        "Terrain Generation" => "Генерация ландшафта",
        "Objects & Spawner" => "Объекты и спавнер",
        "Terrain Brushes" => "Кисти ландшафта",
        "Level Builder" => "Конструктор уровней",
        "Terrain Layers" => "Слои ландшафта",
        "Materials" => "Материалы",
        "Prefabs" => "Префабы",
        "Node Graph" => "Граф нод",
        "Modules" => "Модули",
        "Select" => "Выбор",
        "Ready" => "Готово",
        "Terrain Brush" => "Кисть рельефа",
        "Select / Move / Rotate / Scale gizmo" => "Выбор / Перемещение / Вращение / Масштаб",
        "Sculpt, paint pines, erase props" => "Лепка, посадка сосен, удаление объектов",
        "Rooms, walls, doors, stairs on a snap grid" => "Комнаты, стены, двери, лестницы по сетке",
        "Props" => "Объекты",
        "Pine" => "Сосна",
        "Boulder" => "Валун",
        "Campfire" => "Костёр",
        "Pier" => "Пирс",
        "Lantern" => "Фонарь",
        "Duck" => "Утка",
        "Stone Dungeon" => "Каменное подземелье",
        "Wood Tavern" => "Деревянная таверна",
        "Castle Brick" => "Замковый кирпич",
        "Room Box" => "Комната",
        "Wall Line" => "Стена",
        "Floor Brush" => "Кисть пола",
        "Eraser" => "Ластик",
        "Place Door" => "Дверь",
        "Place Arch" => "Арка",
        "Place Stairs" => "Лестница",
        "Place Pillar" => "Колонна",
        "Place Torch" => "Факел",
        // inspector sections
        "Terrain & Erosion" => "Ландшафт и эрозия",
        "Biome Foliage" => "Растительность биома",
        "Lake & Water" => "Озеро и вода",
        "Sun & Atmosphere" => "Солнце и атмосфера",
        "Fog & God Rays" => "Туман и лучи",
        "Quality" => "Качество",
        // world parameters
        "empty grid scene" => "пустая сетка",
        "seed" => "сид",
        "size" => "размер",
        "resolution" => "разрешение",
        "base height" => "базовая высота",
        "mountain height" => "высота гор",
        "max height" => "макс. высота",
        "noise frequency" => "частота шума",
        "domain warp" => "искажение домена",
        "erosion droplets" => "капли эрозии",
        "thermal iterations" => "термальные итерации",
        "forest density" => "плотность леса",
        "boulder density" => "плотность валунов",
        "grass density" => "плотность травы",
        "lake x" => "озеро X",
        "lake z" => "озеро Z",
        "lake radius" => "радиус озера",
        "lake depth" => "глубина озера",
        "water level" => "уровень воды",
        "shore foam" => "пена у берега",
        "camp x" => "лагерь X",
        "camp z" => "лагерь Z",
        "clearing radius" => "радиус поляны",
        "time of day" => "время суток",
        "auto sun" => "авто-солнце",
        "sun speed" => "скорость солнца",
        "sun intensity" => "яркость солнца",
        "shadows" => "тени",
        "bloom" => "свечение",
        "reflections" => "отражения",
        "fog" => "туман",
        "fog density" => "плотность тумана",
        "fog color" => "цвет тумана",
        "god rays" => "лучи",
        // terrain brush
        "Brush" => "Кисть",
        "Raise" => "Поднять",
        "Lower" => "Опустить",
        "Smooth" => "Сгладить",
        "Flatten" => "Выровнять",
        "Paint Pines" => "Сажать сосны",
        "Erase Props" => "Стирать объекты",
        "Brush Radius (m)" => "Радиус кисти (м)",
        "Strength" => "Сила",
        // terrain surface slots
        "Terrain master" => "Мастер-поверхность",
        "Sand" => "Песок",
        "Gravel" => "Гравий",
        "Grass" => "Трава",
        "Moss" => "Мох",
        "Rock" => "Скала",
        "Snow" => "Снег",
        _ => return None,
    };
    Some(ru)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tr_follows_language_and_defaults_to_english() {
        set(Lang::En);
        assert_eq!(tr("Assets", "Ассеты"), "Assets");
        assert_eq!(label("Terrain Brushes"), "Terrain Brushes");
        assert_eq!(shown("mountain height"), "mountain height");
        set(Lang::Ru);
        assert_eq!(tr("Assets", "Ассеты"), "Ассеты");
        assert_eq!(label("Terrain Brushes"), "Кисти ландшафта");
        assert_eq!(label("Quillshade"), "Quillshade");
        assert_eq!(shown("mountain height"), "высота гор");
        set(Lang::En);
    }
}