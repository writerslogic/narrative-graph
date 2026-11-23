import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import sample_training
import teacher


class TeacherItems(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.sample = Path(cls.directory.name) / "sample.txt"
        with patch.object(sys, "argv", ["sample_training.py", str(cls.sample), "--per-book", "1"]):
            sample_training.main()
        _, cls.items = teacher.parse_items(cls.sample.read_text(encoding="utf-8"))
        _, cls.evaluated = teacher.parse_items((teacher.PUBLIC / "labels.txt").read_text(encoding="utf-8"))

    def test_training_defaults_to_all_and_reads_the_public_guide(self):
        out = Path(self.directory.name) / "answers.txt"
        inputs = Path(self.directory.name) / "with-header.txt"
        sample = self.sample.read_text(encoding="utf-8").partition("\n@ ")[2]
        inputs.write_text(f"# arbitrary user header\n@ {sample}", encoding="utf-8")
        argv = ["teacher.py", str(teacher.PUBLIC), str(out), "--items", str(inputs)]
        with patch.object(sys, "argv", argv), patch.object(teacher, "ask", return_value="none") as ask:
            teacher.main()
        self.assertEqual(ask.call_count, len(self.items))
        for call in ask.call_args_list:
            prompt = call.args[1]
            self.assertIn("=== GUIDE ===\nHand labels", prompt)
            self.assertNotIn("arbitrary user header", prompt)
        for item in self.items:
            self.assertIn(f"@ {item['number']}\nnone", out.with_suffix(".txt.raw.txt").read_text())

    def test_rejects_unverified_text_and_overlapping_windows(self):
        item = self.items[0]
        evaluated = self.evaluated[0]
        book = evaluated["book"]
        text = (teacher.PUBLIC / f"{book}.txt").read_text(encoding="utf-8")
        lo, hi, previous, sentence = dict(sample_training.sentence_windows(text))[evaluated["start"]]
        overlap = dict(evaluated, number=10000, context=" ".join(previous.split()), sentence=" ".join(sentence.split()))
        cases = [
            ([dict(item, book="../private/1")], "public novels"),
            ([dict(item, start=item["start"] + 1)], "byte offset"),
            ([dict(item, context="different context")], "differs from the source"),
            ([dict(item, sentence="different target")], "differs from the source"),
            ([dict(item, number=self.evaluated[0]["number"])], "repeated item"),
            ([item, dict(item, number=10000)], "repeated item"),
            ([overlap], "overlaps an evaluation item"),
        ]
        for items, error in cases:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                teacher.training_books(teacher.PUBLIC, items, self.evaluated)
        self.assertEqual([sample_training.overlaps(lo - 5, end, [(lo, hi)])
                          for end in (lo - 1, lo, lo + 1)], [False, False, True])

    def test_wider_window_is_refused_before_request(self):
        out = Path(self.directory.name) / "batch.txt"
        argv = ["teacher.py", str(teacher.PUBLIC), str(out), "--items", str(self.sample),
                "--provider", "openai", "--batch", "submit", "--window", str(sample_training.WINDOW // 2 + 1)]
        with patch.object(sys, "argv", argv), patch.object(teacher, "openai") as request:
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                teacher.main()
            request.assert_not_called()

    def test_batch_rejects_a_different_selection_before_download(self):
        state = Path(self.directory.name) / "saved.batch.json"
        state.write_text(json.dumps({"batch": "local", "selection": "submitted"}), encoding="utf-8")
        with patch.object(teacher, "openai") as request:
            with self.assertRaisesRegex(SystemExit, "differ from the submitted batch"):
                teacher.collect_batch(state, "changed", require_selection=True)
            request.assert_not_called()


if __name__ == "__main__":
    unittest.main()
