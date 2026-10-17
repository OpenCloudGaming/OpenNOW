import importlib.util
import pathlib
import struct
import unittest


spec = importlib.util.spec_from_file_location("gfn_inventory", pathlib.Path(__file__).with_name("gfn_inventory.py"))
inventory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inventory)


class MachOInventoryTests(unittest.TestCase):
    def thin(self, cpu=0x1000007):
        return struct.pack("<8I", 0xFEEDFACF, cpu, 3, 6, 0, 0, 0, 0)

    def test_thin_identity(self):
        data = self.thin()
        self.assertEqual(inventory.slices(data), [(0x1000007, 0, data)])
        self.assertEqual(inventory.macho(data)["kind"], 6)

    def test_fat_identity_and_container_offsets(self):
        x86, arm = self.thin(), self.thin(0x100000C)
        data = struct.pack(">7I", 0xCAFEBABE, 1, 0x1000007, 3, 28, len(x86), 0) + x86
        self.assertEqual(inventory.slices(data), [(0x1000007, 28, x86)])
        wide = struct.pack(">II", 0xCAFEBABF, 1) + struct.pack(">IIQQII", 0x100000C, 0, 40, len(arm), 0, 0) + arm
        self.assertEqual(inventory.slices(wide), [(0x100000C, 40, arm)])

    def test_rejects_out_of_bounds_slice(self):
        data = struct.pack(">7I", 0xCAFEBABE, 1, 0x1000007, 3, 128, 32, 0)
        with self.assertRaises(ValueError):
            inventory.slices(data)

    def test_rejects_out_of_bounds_commands(self):
        data = struct.pack("<8I", 0xFEEDFACF, 0x1000007, 3, 6, 1, 8, 0, 0)
        with self.assertRaises(ValueError):
            inventory.macho(data)

    def test_rejects_invalid_command_length(self):
        data = struct.pack("<8I", 0xFEEDFACF, 0x1000007, 3, 6, 1, 8, 0, 0) + struct.pack("<II", 2, 0)
        with self.assertRaises(ValueError):
            inventory.macho(data)

    def test_non_macho_has_no_slices(self):
        self.assertEqual(inventory.slices(b"symlink placeholder"), [])


if __name__ == "__main__":
    unittest.main()
