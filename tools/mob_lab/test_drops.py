import importlib.util
import unittest
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "mob_lab_server", Path(__file__).resolve().parent / "server.py"
)
server = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(server)


class DropValidationTests(unittest.TestCase):
    def limits(self):
        return {30015: 20, 30006: 1}

    def test_independent_rows_and_bounds(self):
        errors = server.validate_drop_list(
            [
                {"item": 30015, "chance_bps": 1000, "quantity_min": 1, "quantity_max": 3},
                {"item": 30006, "chance_bps": 10000, "quantity_min": 1, "quantity_max": 1},
            ],
            self.limits(),
        )
        self.assertEqual(errors, [])

    def test_duplicate_unknown_and_stack_are_rejected(self):
        errors = server.validate_drop_list(
            [
                {"item": 30015, "chance_bps": 10001, "quantity_min": 1, "quantity_max": 21},
                {"item": 30015, "chance_bps": 0, "quantity_min": 1, "quantity_max": 1},
                {"item": 9, "chance_bps": 0, "quantity_min": 1, "quantity_max": 1},
                {"item": 30006, "chance_bps": 10000, "quantity_min": 2, "quantity_max": 2},
            ],
            self.limits(),
        )
        self.assertGreaterEqual(len(errors), 4)

    def test_omitted_drops_stay_valid(self):
        self.assertEqual(server.validate_drop_list([], self.limits()), [])


if __name__ == "__main__":
    unittest.main()
