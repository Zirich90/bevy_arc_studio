import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('sync_memory', Path(__file__).resolve().parents[1] / 'sync_memory.py')
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)

class MemoryTests(unittest.TestCase):
    def test_export_is_repeatable_and_preserves_manual_notes(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / 'target') as temporary:
            root = Path(temporary)
            (root / 'graphify-out').mkdir()
            memory = root / 'memory'
            memory.mkdir()
            manual = memory / '00_STATE.md'
            manual.write_text('user state', encoding='utf-8')
            (root / 'graphify-out/graph.json').write_text(json.dumps({'nodes': [
                {'id': 'a', 'label': 'Root', 'source_file': 'src/prefabs.rs'},
                {'id': 'b', 'label': 'Variant', 'source_file': 'src/prefab_variants.rs'}
            ], 'edges': [{'source': 'a', 'target': 'b', 'relation': 'calls'}]}), encoding='utf-8')
            self.assertEqual(sync.export_graph(root, memory), (2, 1))
            before = {str(p.relative_to(memory)): p.read_bytes() for p in memory.rglob('*') if p.is_file()}
            self.assertEqual(sync.export_graph(root, memory), (2, 1))
            after = {str(p.relative_to(memory)): p.read_bytes() for p in memory.rglob('*') if p.is_file()}
            self.assertEqual(before, after)
            self.assertEqual(manual.read_text(), 'user state')
            self.assertIn('[[ArcStudio/Graph/', (memory / 'Graph/00_INDEX.md').read_text(encoding='utf-8'))

    def test_note_names_are_unique_on_windows(self):
        names = sync.note_paths([{'id': 'a', 'label': 'Node', 'source_file': 'src/prefabs.rs'},
                                 {'id': 'b', 'label': 'node', 'source_file': 'src/prefabs.rs'}])
        self.assertEqual(len({n.casefold() for n in names.values()}), 2)

    def test_generated_paths_cannot_escape_graph(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / 'target') as temporary:
            with self.assertRaises(ValueError):
                sync.graph_path(Path(temporary), '../user.md')

if __name__ == '__main__':
    unittest.main()
