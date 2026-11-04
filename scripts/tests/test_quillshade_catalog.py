import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('quillshade_catalog', Path(__file__).parents[1] / 'quillshade_catalog.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
test_temp = Path(__file__).resolve().parents[2] / 'target'
test_temp.mkdir(exist_ok=True)

class CatalogTests(unittest.TestCase):
    def test_uniform_access_and_layout_come_from_generated_types(self):
        with tempfile.TemporaryDirectory(dir=test_temp) as temp:
            root = Path(temp)
            source = root / 'crates/quillshade/src/material/materials'
            source.mkdir(parents=True)
            (root / 'src').mkdir()
            (source / 'water.rs').write_text('pub const BAKED_ID: &str = "water";\npub const LAYOUT_FEATURE: &str = "SG_LAYOUT_test";\npub struct WaterParams { pub inp_depth: f32 }\npub type WaterMaterial = ExtendedMaterial<Base, Ext>;')
            module.catalog(root)
            output = (root / 'src/quillshade_catalog.rs').read_text()
            self.assertIn('Some(&mut m.extension.params)', output)
            self.assertIn('WaterParams::default()', output)
            self.assertIn('::LAYOUT_FEATURE', output)
            self.assertIn('pub fn uniforms', output)

    def test_generated_alias_and_unlit_are_included_but_sprite_is_not(self):
        with tempfile.TemporaryDirectory(dir=test_temp) as temp:
            root = Path(temp)
            source = root / 'crates/quillshade/src/material/materials'
            source.mkdir(parents=True)
            (root / 'src').mkdir()
            (source / 'water.rs').write_text('pub const BAKED_ID: &str = "water";\npub type WaterMaterial = ExtendedMaterial<Base, Ext>;')
            (source / 'vfx.rs').write_text('pub const BAKED_ID: &str = "vfx";\npub struct VfxMaterial {}')
            (source / 'sprite.rs').write_text('use MaterialExtension2d;\npub const BAKED_ID: &str = "sprite";\npub struct SpriteMaterial {}')
            self.assertEqual(module.catalog(root), 2)
            output = (root / 'src/quillshade_catalog.rs').read_text()
            self.assertIn('WaterMaterial', output)
            self.assertIn('VfxMaterial', output)
            self.assertNotIn('SpriteMaterial', output)

    def test_incompatible_output_does_not_replace_previous_catalog(self):
        with tempfile.TemporaryDirectory(dir=test_temp) as temp:
            root = Path(temp)
            (root / 'src').mkdir()
            output = root / 'src/quillshade_catalog.rs'
            output.write_text('previous')
            with self.assertRaises(ValueError):
                module.catalog(root)
            self.assertEqual(output.read_text(), 'previous')

if __name__ == '__main__':
    unittest.main()
