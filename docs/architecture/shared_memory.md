# Общая память ArcStudio

Vault: `E:/Program Files/Obsidian/AI Memori`, папка `ArcStudio`.
`.memory` — проверяемый Windows junction. Он и `graphify-out` исключены из Git.

При старте и восстановлении контекста читать `00_STATE.md`, `01_BLACKBOARD.md`, нужные части `02_DECISIONS.md`.
Состояние держать до 4 КБ: текущая задача, следующий шаг, результаты проверок и живые handles.
Доска содержит одну активную задачу. Решения содержат только принятые ограничения.

Настройка и повторный экспорт (Python 3.12+):

```powershell
python scripts/sync_memory.py --vault 'E:/Program Files/Obsidian/AI Memori'
python scripts/sync_memory.py
```

Скрипт адаптирован из BIMCraft: сохраняет ручные заметки, создаёт начальные файлы только при их отсутствии,
проверяет назначение junction, резервирует заменяемые generated notes в `target/memory-graph-backups`.
Папка BIMCraft не меняется. Qwen и Stop hooks не подключены.

Граф перестраивается отдельно в окружении с установленным `graphifyy`:

```powershell
python scripts/build_memory_graph.py
graphify export html
python scripts/sync_memory.py
```

`build_memory_graph.py` извлекает только исходники `src/` через AST. Документы и ассеты не индексируются.
HTML: `graphify-out/graph.html`. JSON: `graphify-out/graph.json`. Отчёт: `graphify-out/GRAPH_REPORT.md`.
Obsidian: `ArcStudio/Graph/00_INDEX.md`, группы core/editor/ui/external, ссылки между узлами.
Это снимок исходников, не доказательство актуальности. Проверять текущие строки и SHA-256.
Последний снимок: 1894 узла, 4795 связей, 270 dangling endpoints, 569 collapsed undirected edges; полный анализ в `health.json`.
Отсутствующий узел требует точечного поиска в текущих исходниках. Не заявлять полное покрытие или измеренное ускорение.
