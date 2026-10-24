# 08. Команды редактора (`StudioCommand`), базовая линия E0

Источник: `src/mcp_commands.rs`. Один канал для UI и ИИ: `StudioCommandQueue` (BRP-ресурс), обработка раз в кадр.
Схема операторов с параметрами: команда `Describe`. Результат каждой команды — строка в `StudioCommandQueue.log`.

| Группа | Команды | Откат |
|---|---|---|
| Режим и вид | `SetMode`, `SetPlayMode { mode: Edit|Playing|Paused }` (снимок и откат сцены сразу, результат в `log`), `SetTab`, `SetQuality`, `ApplyPreset`, `SetTime` | нет |
| Камера | `SetCameraStyle`, `SetCamera`, `FocusSelected` | нет |
| Террейн | `SetBrush`, `Sculpt` | да |
| Мир | `SetSpec { field, value }` (поле `WorldSpec` по reflect-пути) | да |
| Объекты | `SpawnObject`, `DeleteSelected`, `Duplicate`, `SetTransform` | да |
| Выделение | `Select { name }`, `SelectIds`, `ListObjects`, `DescribeScene` | нет |
| Гизмо | `SetGizmo` | нет |
| Компоненты | `SetComponent`, `AddComponent`, `RemoveComponent` | да |
| Иерархия | `Reparent` | да (E1) |
| История | `Undo`, `Redo` | — |
| Сцена и снимок | `SaveScene`, `LoadScene`, `Screenshot` | — |
| Навигация | `SpawnActor`, `QueryPath`, `MoveActor` | нет |
| Самоописание | `Describe` | — |
| pf2e (feature) | `SpawnActorFromTemplate` | нет |

История (E1): четыре стека (World/Scene/Terrain/Comp) объединены глобальным порядком `UndoOrder`; свежая правка сбрасывает redo всех. Разрыв: снимки компонентов держат `Entity`, пересоздание пропсов (`SetSpec` террейна/озера) делает записи мёртвыми.
