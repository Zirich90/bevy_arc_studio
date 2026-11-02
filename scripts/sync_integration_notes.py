"""Publish additive integration notes into the existing ArcStudio Obsidian folder."""
from pathlib import Path
import hashlib
import argparse

ROOT = Path(__file__).resolve().parents[1]
START, END = '<!-- arc-integration:start -->', '<!-- arc-integration:end -->'

def update(path, content):
    original = path.read_text(encoding='utf-8') if path.exists() else ''
    block = f'{START}\n{content.rstrip()}\n{END}'
    if START in original:
        before, tail = original.split(START, 1)
        _, after = tail.split(END, 1)
        result = before + block + after
    else:
        result = original.rstrip() + '\n\n' + block + '\n'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(result, encoding='utf-8')

def sync(vault):
    studio = vault / 'ArcStudio'
    if not (studio / '00_STATE.md').is_file():
        raise ValueError('Existing ArcStudio memory required; no duplicate bevy_arc_studio folder will be created')
    link = '[[ArcStudio/Graph/integration_quillshade_noodle]]'
    update(studio / '00_STATE.md', f'''## 2026-11-02: integration stage 09
Каркас Quillshade/Noodle подключён; полная задача не принята. {link}.
cargo check --tests PASS; Python catalog 2 tests PASS; импорт 140 файлов без правок.
Память исправлена: активная подкачка E:/pagefile.sys 32–64 ГиБ; C: сохранён. Backup/результат: target/pagefile-*.json. 142 Rust-теста PASS, 1 doctest ignored; студия запущена и штатно закрыта через BRP.
BRP PASS: 11 материалов, Undo/Redo, graph PIE и scene roundtrip prefab root + tile. Три скриншота просмотрены. Следующее: I2 — исполнение графов и мультифизика, недостающие material contracts и полная приёмка.
README обновлён EN/RU: upstream GitHub links, snapshots, возможности и ограничения. Владелец разрешил публикацию каркаса с переносом отсутствующего runtime в следующий этап. Поиск в обоих текущих upstream HEAD не нашёл backend мультифизики; geometry_nodes — пример точек и инстансинга. LIVE: процессов студии и живых handles нет. Результат публикации фиксируется отдельной записью состояния.''')
    update(studio / '01_BLACKBOARD.md', f'''## 2026-11-02: Quillshade / Noodle integration
Активная работа: {link}. Контракты и пошаговый план: E:/bevy_arc_studio/docs/architecture/08_quillshade_noodle.md.
Подключены generated runtime, typed material dispatch, native canvas, StudioCommand, graph Undo/Redo и graph PIE/scene snapshot.
142 Rust-теста PASS, 1 doctest ignored; BRP assign/Undo/Redo, graph PIE, prefab root/tile scene roundtrip PASS. OOM/mmap 1455 исправлен активной подкачкой на E:. Скриншоты integration_world_graph/event_graph/materials.png просмотрены; полный этап не принят.
Статус приёмки и результаты: docs/clean_room_audit/logs/stage_09_integration_log.md.
Следующий этап I2: backend симуляции, typed material parameters, surface slots, environment/member persistence и evaluators мультифизики/PF2e; полная визуальная/hot-reload приёмка. Поиск в текущих исходниках авторов не обнаружил runtime Biology/Granular/FluidFlow. См. доказательства и критерии I2 в архитектурном документе.
Названия нод не означают готовый симулятор. Biology/Granular/FluidFlow runtime ещё отсутствует.''')
    update(studio / '02_DECISIONS.md', f'''## 2026-11-02: контракт интеграции
- Решение владельца: публиковать проверенный каркас; отсутствующий в исходниках авторов runtime перенести в I2. geometry_nodes Noodle использовать как пример point processing/instancing, не как готовую мультифизику.
- Требование владельца: все сборки только на E:. Cargo target = E:/bevy_arc_studio/target; TEMP/TMP = target/tmp через .cargo/config.toml; временные флаги профиля не меняют версии стека.
- По запросу владельца добавлена подкачка E:/pagefile.sys 32768–65536 MiB; C: сохранён. Штатные профили Cargo оставлены; opt-level=0 для PBR не использовать из-за лимита экспортов DLL. Откат через configure_build_pagefile.ps1 -Action Restore с сохранённым backup.
- Quillshade RON — source of truth; generated crates/quillshade, WESL и baked.ron не редактировать.
- Studio adapter и catalog generation живут вне output; watcher меняет данные в прежнем SG_LAYOUT, структура требует сборки.
- Noodle upstream 0.5.0 отдельно от fork Quillshade; canvas не владеет domain data.
- StudioGraph: versioned RON, stable u32 ids, domain ports, DAG, finite values, limits; pure validation в arc_core.rs.
- EditApplied записывает финал жеста в ComponentHistory/UndoOrder; replay не создаёт историю; PIE восстанавливает модель графа.
- WPO — GPU visual only; физика и Theta* используют CPU state и geometry revision.
- PF2e effect публикуется после успешного каста; единицы через UnitScale. Simulation runtime и energy mapping требуют следующего этапа.
- Per-layer terrain и per-basin water требуют shader/mesh contracts; текущий binding объекта не считать их реализацией.
Схема, ограничения и критерии: {link}.''')
    source = ROOT / 'docs/architecture/08_quillshade_noodle.md'
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    update(studio / 'Graph/integration_quillshade_noodle.md', f'''# Quillshade / Noodle integration
Evidence: EXTRACTED implementation + PROPOSED runtime plan, 2026-11-02.
Source: {source.as_posix()}; SHA256 {digest}.
[[ArcStudio/01_BLACKBOARD]] · [[ArcStudio/02_DECISIONS]]
[[bevy_quillshade/Graph/generated_crate]] · [[bevy_quillshade/Graph/hot_reload]] · [[bevy_quillshade/Graph/extended_material]]
[[bevy_noodle/Graph/EditApplied]] · [[bevy_noodle/Graph/ConnectionCheck]] · [[bevy_noodle/Graph/evaluation]] · [[bevy_noodle/Graph/undo_redo]]

{source.read_text(encoding='utf-8')}''')
    update(studio / 'Graph/00_INDEX.md', f'## Integration\n{link} — generated materials, native canvas, contracts and staged runtime plan.')
    for module in ('bevy_quillshade', 'bevy_noodle'):
        update(vault / module / 'Graph/00_INDEX.md', f'## ArcStudio integration\n{link} — Studio adapter, ownership, RON/BRP contracts and runtime roadmap.')
    print('Updated ArcStudio BLACKBOARD/DECISIONS/Graph and module index backlinks; BIMCraft untouched')

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--vault', type=Path, default=Path('E:/Program Files/Obsidian/AI Memori'))
    sync(parser.parse_args().vault)
