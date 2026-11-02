# Stage 09 — user prompt, 2026-11-02

Follow-up constraint: «все сборки только на диск E».
Follow-up repair request: «разберись с памятью. раньше собиралась».

Задача: Спроектировать и организовать интеграцию нашей студии (`bevy_arc_studio` + модуль мультифизической симуляции террейна) с двумя модулями из хранилища Obsidian (`AI Memori`): `bevy_quillshade` и `bevy_noodle`.

Изучи их заметки в Obsidian (`00_INDEX`, `02_DECISIONS`, `Graph`) и реализуй архитектуру интеграции по трем направлениям:

1. **Мост с `bevy-quillshade` (Материалы, шейдеры WESL и освещение):**
   - Настрой пайплайн односторонней кодогенерации Quillshade так, чтобы сгенерированный им крейт (`crates/quillshade`) и шейдеры бесшовно подключались в воркспейс `bevy_arc_studio` с поддержкой Hot Reload (`file_watcher`).
   - Пробрось материалы из реестра `Quillshade` в наш `EditorRegistry` и универсальный Reflect-инспектор, чтобы любые нодовые материалы (вода, лава, лед, горящая нефть, трава с `World Position Offset`, VFX заклинаний для `bevy_arc_pf2e`) можно было назначать слоям террейна, тайлам `Level Builder`, водоемам (`SetWaterBody`) и префабам прямо из UI студии.
   - Добавь команды управления материалами и пресетами окружения Quillshade в протокол `StudioCommand` (`bevy_remote` / BRP), чтобы ИИ-агент мог менять материалы и проверять картинку через скриншоты.

2. **Встраивание `bevy_noodle` (Визуальный нодовый редактор внутри студии):**
   - Интегрируй `bevy_noodle` как нативный режим/панель внутри `bevy_arc_studio`.
   - **Пайплайн 1 — Процедурный мир и мультифизика:** Сделай нодовый граф управления генерацией террейна и правилами симуляции (Шум -> Гидравлическая эрозия -> Температурные пороги льда/снега -> Потоки воды/лавы/нефти -> Рост растительности `Biology` и спавн объектов по типу `geometry_nodes`).
   - **Пайплайн 2 — Логика и интерактивность (`bevy_arc_pf2e` + физика среды):** Сделай ноды для связи боевых действий с физикой мира (например: каст `Fireball` в бою -> триггер воспламенения разлитой нефти / таяния снега / обрушения лавины `Granular` с пересчетом навмеша `Theta*`).
   - Свяжи события редактирования графа (`On<EditApplied>` из `bevy_noodle`) с нашим единым транзакционным стеком `Undo/Redo` и системой снапшотов `Play-in-Editor (PIE)`.

3. **Документация в Obsidian:**
   - Зафиксируй схему интеграции, контракты данных (`.ron` / `StudioCommand`) и пошаговый план работ в `bevy_arc_studio/01_BLACKBOARD.md` и `bevy_arc_studio/02_DECISIONS.md`, обновив связи в графе `Graph/`.

Audit note: prompt persisted after initial inspection and first implementation edits, rather than before them. This timing deviation is recorded explicitly.

Owner follow-up: если все готово то коммить и пуш. обязательно обнови ридми и укажи ссылки на использованые гитхаб репозитории
Action: README updated in English/Russian with upstream links and explicit incomplete scope. Conditional commit/push not executed: full integration is not ready.

Owner follow-up: найди в исходниках авторов. если нет, то запиши в следующий этап и пуш
Resume: продолжи. Author source search completed at matching remote HEADs; record missing runtime as next stage and publish tested foundation.
