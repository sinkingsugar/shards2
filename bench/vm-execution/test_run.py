"""Result validation must reject a fast run that did the wrong work."""
import unittest

from run import parse_output


class OutputValidation(unittest.TestCase):
    GOOD = """[log prefix] VM_SECONDS: 0.01
[log prefix] VM_VALID: true
[log prefix] VM_DONE: 100
VM_SECONDS: 2e-2
VM_VALID: true
VM_DONE: 100
main: 0
"""

    def test_both_log_formats(self):
        self.assertEqual(parse_output(self.GOOD, 100, 2), [0.01, 0.02])

    def test_wrong_value(self):
        with self.assertRaises(RuntimeError):
            parse_output(self.GOOD.replace("true", "false", 1), 100, 2)

    def test_wrong_iteration_count(self):
        with self.assertRaises(RuntimeError):
            parse_output(self.GOOD.replace("VM_DONE: 100", "VM_DONE: 99", 1), 100, 2)

    def test_missing_sample(self):
        with self.assertRaises(RuntimeError):
            parse_output(self.GOOD, 100, 3)

    def test_extra_sample(self):
        with self.assertRaises(RuntimeError):
            parse_output(self.GOOD + self.GOOD, 100, 2)

    def test_invalid_time(self):
        for bad in ("0", "-0.01", "1e999", "nan", "inf", "0.01junk"):
            with self.subTest(bad=bad), self.assertRaises(RuntimeError):
                parse_output(self.GOOD.replace("0.01", bad), 100, 2)


if __name__ == "__main__":
    unittest.main()
