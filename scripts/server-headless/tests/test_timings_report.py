"""Regression checks for the privacy-preserving native timing report parser."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('omsi_timings', ROOT / 'timings-report.py')
parser = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parser)


class TimingParserTests(unittest.TestCase):
    def test_preserves_real_measured_timings_and_does_not_copy_log_text(self):
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / 'native.log'
            sample = (
                'user=Alice, IP=192.0.2.55, join code PRIVATE-CODE\n'
                'native load: TPS 59.8/60, 123 active CPU tiles, 160 AI cars, '
                '4 AI buses, 310 pedestrians (100 walking, 200 waiting, 10 aboard); '
                'ground heap est.: 10.0 MiB terrain, 3.2 MiB raster, 1.1 MiB wheel; '
                'tick p95 8.10 ms p99 12.20 ms (profiling on)\n'
                'server console: timings stopped, 36000 ticks in 600.0 s, '
                'tick avg 6.20 ms, p95 9.40 ms, p99 12.20 ms, max 22.00 ms\n'
                'server console: network            mean 0.200 ms, max 2.300 ms, '
                'total 150.0 ms (2.5%), calls 36000\n'
            )
            log.write_text(sample, encoding='utf-8')
            report = parser.make_report(log, None, label='synthetic-test',
                                        sha='test', clients=2)
            self.assertEqual(report['tps']['mean'], 59.8)
            self.assertEqual(report['actual_ai_cars']['max'], 160)
            self.assertEqual(report['latest_timings']['p99_tick_ms'], 12.2)
            self.assertEqual(report['latest_timings']['stages']['network']['calls'], 36000)
            output = json.dumps(report)
            for forbidden in ('Alice', '192.0.2.55', 'PRIVATE-CODE'):
                self.assertNotIn(forbidden, output)

    def test_missing_data_is_null_not_fabricated_zero(self):
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / 'empty.log'
            log.write_text('no metric records here\n', encoding='utf-8')
            report = parser.make_report(log, None, label='none', sha='none',
                                        clients=None)
            self.assertIsNone(report['latest_timings'])
            self.assertIsNone(report['tps'])
            self.assertIsNone(report['rss_mib'])


if __name__ == '__main__':
    unittest.main()
