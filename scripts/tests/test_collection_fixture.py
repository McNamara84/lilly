"""Exercise the benchmark oracle independently of MariaDB and production SQL."""

import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import collection_fixture as fixture


class FixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalogue = fixture.issues()
        cls.collection = fixture.entries(cls.catalogue)

    def test_exact_data_sizes_and_copy_distribution(self):
        self.assertEqual(len(self.catalogue), 7000)
        self.assertEqual(len(self.collection), 10000)
        self.assertEqual(len({r["issue_id"] for r in self.collection}), 5000)
        self.assertEqual({r["series_slug"] for r in self.collection}, {"maddrax", "john-sinclair"})
        self.assertEqual({r["condition_grade"] for r in self.collection}, {None, "Z0", "Z1", "Z2", "Z3", "Z4"})
        self.assertEqual({r["copy_number"] for r in self.collection}, {1, 2, 3})

    def test_result_oracle_has_known_counts_and_stable_ties(self):
        result = fixture.expected(self.catalogue, self.collection, {})
        self.assertEqual((result["total"], result["page"], result["per_page"]), (10000, 1, 50))
        self.assertEqual([r["series_slug"] for r in result["data"][:2]], ["john-sinclair", "maddrax"])
        for params, total in [({"series_slug": "maddrax"}, 5000),
                              ({"series_slug": "john-sinclair", "status": "missing"}, 1000),
                              ({"series_slug": "maddrax", "status": "missing"}, 1000),
                              ({"author": "Cover Artist Only"}, 0),
                              ({"q": "does-not-exist"}, 0), ({"issue_id": "7001"}, 0)]:
            with self.subTest(params=params):
                self.assertEqual(fixture.expected(self.catalogue, self.collection, params)["total"], total)

    def test_all_scenarios_are_unique_and_cover_the_contract(self):
        scenarios = fixture.scenarios()
        self.assertEqual(len(scenarios), len({name for name, _ in scenarios}))
        self.assertGreaterEqual(len(scenarios), 60)
        for name, params in scenarios:
            with self.subTest(name=name):
                result = fixture.expected(self.catalogue, self.collection, params)
                fixture.validate_response(result, result)
                self.assertLessEqual(len(result["data"]), 100)
                self.assertEqual(len({(r["id"], r["issue_id"]) for r in result["data"]}), len(result["data"]))
        for field in fixture.SORTS:
            for direction in ("asc", "desc"):
                self.assertIn((f"sort-{field}-{direction}", {"sort": field, "sort_dir": direction}), scenarios)

    def test_collation_and_like_wildcards(self):
        for value, query, match in [("Müller", "muller", True), ("Café", "cafe", True),
                                     ("ABCD", "a_d", False), ("ABCD", "a%d", True),
                                     ("ABCD", "b_c", False), ("A\nC", "a_c", True),
                                     ("ABCD", "  bc  ", True), ("ABCD", "z", False)]:
            with self.subTest(value=value, query=query):
                self.assertEqual(fixture.contains(value, query), match)

    def test_pagination_clamps_values_and_returns_empty_outside_collection(self):
        for params, page, size, count in [({"page": "0", "per_page": "0"}, 1, 1, 1),
                                        ({"per_page": "500"}, 1, 100, 100),
                                        ({"page": "200"}, 200, 50, 50),
                                        ({"page": "201"}, 201, 50, 0),
                                        ({"page": "4294967295", "per_page": "100"}, 4294967295, 100, 0)]:
            with self.subTest(params=params):
                result = fixture.expected(self.catalogue, self.collection, params)
                self.assertEqual((result["page"], result["per_page"], len(result["data"])), (page, size, count))

    def test_no_rows_are_lost_or_repeated_between_pages(self):
        for field in fixture.SORTS:
            ordered = []
            for page in range(1, 101):
                result = fixture.expected(self.catalogue, self.collection,
                                          {"sort": field, "sort_dir": "desc", "page": str(page), "per_page": "100"})
                ordered.extend(r["id"] for r in result["data"])
            self.assertEqual(len(ordered), 10000)
            self.assertEqual(set(ordered), set(range(1, 10001)))

    def test_missing_entries_have_no_copy_metadata_and_authors_are_not_duplicated(self):
        result = fixture.expected(self.catalogue, self.collection, {"status": "missing", "series_slug": "maddrax"})
        self.assertTrue(all(r["id"] == 0 and r["status"] == "missing" and r["copy_number"] is None for r in result["data"]))
        result = fixture.expected(self.catalogue, self.collection, {"issue_id": "7", "author": "muller"})
        self.assertEqual(result["total"], 1)
        result = fixture.expected(self.catalogue, self.collection, {"title": " ", "author": " "})
        self.assertEqual(result["total"], 10000)

    def test_condition_range_and_combined_filters(self):
        params = {"condition_min": "Z1", "condition_max": "Z3", "status": "owned", "author": "muller"}
        result = fixture.expected(self.catalogue, self.collection, params)
        self.assertGreater(result["total"], 0)
        self.assertTrue(all("Z1" <= r["condition_grade"] <= "Z3" and r["status"] == "owned" for r in result["data"]))

    def test_bad_results_are_rejected_including_wrong_user_and_order(self):
        expected = fixture.expected(self.catalogue, self.collection, {})
        changes = [lambda x: x.update(total=0), lambda x: x.update(page=True),
                   lambda x: x.update(per_page=101), lambda x: x.update(data=None),
                   lambda x: x["data"].pop(), lambda x: x["data"].reverse(),
                   lambda x: x["data"][0].update(id=10001),
                   lambda x: x["data"][0].pop("notes"),
                   lambda x: x["data"].__setitem__(0, "invalid")]
        for change in changes:
            with self.subTest(change=change):
                actual = copy.deepcopy(expected)
                change(actual)
                with self.assertRaises(ValueError):
                    fixture.validate_response(actual, expected)
        with self.assertRaises(ValueError):
            fixture.validate_response([], expected)

    def test_manifest_is_stable_and_changes_with_data(self):
        manifest = fixture.manifest(self.catalogue, self.collection)
        self.assertEqual(manifest, fixture.manifest(self.catalogue, self.collection))
        changed = copy.deepcopy(self.collection)
        changed[0]["notes"] = "changed"
        self.assertNotEqual(manifest["sha256"], fixture.manifest(self.catalogue, changed)["sha256"])
        self.assertEqual(manifest["foreign_entries"], 100000)

    def test_sql_literals_do_not_interpret_quotes_or_shell_characters(self):
        self.assertEqual(fixture.literal(None), "NULL")
        self.assertEqual(fixture.literal(123), "123")
        for value in ["Müller", "'; DROP TABLE users; --", "`$()\\\n"]:
            literal = fixture.literal(value)
            self.assertNotIn(value, literal)
            self.assertEqual(bytes.fromhex(literal.split("0x")[1].split()[0]).decode(), value)
        self.assertEqual(list(fixture.insert_batches("t", ("a",), [], 2)), [])
        self.assertEqual(len(list(fixture.insert_batches("t", ("a",), [(1,), (2,), (3,)], 2))), 2)

    def test_sql_seed_is_transactional_and_foreign_count_is_exact(self):
        own = list(fixture.fixture_sql(self.catalogue, self.collection))
        foreign = list(fixture.foreign_sql())
        for sql in (own, foreign):
            self.assertEqual((sql[0], sql[-1]), ("START TRANSACTION;", "COMMIT;"))
        self.assertEqual(sum(s.count("),(") + 1 for s in foreign if s.startswith("INSERT INTO collection_entries")), 100000)
        self.assertEqual(sum(s.count("),(") + 1 for s in foreign if s.startswith("INSERT INTO users")), 100)


if __name__ == "__main__":
    unittest.main()
