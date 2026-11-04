"""Student README contract: benefit, steps, captures, no social links."""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
README = (ROOT / "README.md").read_text(encoding="utf-8")


class ReadmeTests(unittest.TestCase):
    def test_student_guide_has_a_benefit_and_ai_steps_without_social_links(self):
        self.assertIn("without typing your school password into the chat", README)
        for name in ("Claude", "ChatGPT", "OpenCode"):
            self.assertIn(name, README)
        self.assertNotIn("experimental", README.lower())
        self.assertIn("tools/GUIDE.md", README)
        self.assertNotRegex(README, r"https?://(www\.)?(x|twitter)\.com")
        for image in ("home.png", "moodle.png", "link.png", "omnivox.png"):
            path = ROOT / "tools" / "screens" / image
            self.assertTrue(path.is_file(), image)
            self.assertGreater(path.stat().st_size, 1000)
            self.assertIn(f"tools/screens/{image}", README)
        self.assertTrue(re.search(r"Connect with browser", README))
        self.assertNotIn("SPEC.md", README)


if __name__ == "__main__":
    unittest.main()
