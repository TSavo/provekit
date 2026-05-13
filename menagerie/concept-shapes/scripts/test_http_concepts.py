import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_http_concepts as http


class HttpConceptShapeTests(unittest.TestCase):
    def test_http_shapes_have_required_slots_and_loss_records(self):
        specs = http.build_shape_specs()
        request = specs["http-request"]
        response = specs["http-response"]

        self.assertEqual(request["fn_name"], "concept:http-request")
        self.assertEqual(
            request["formals"],
            ["method", "url", "headers", "body"],
        )
        self.assertEqual(request["post"]["operator"], "http-request")
        self.assertEqual(
            sorted(request["post"]["loss_record"]),
            sorted(http.HTTP_REQUEST_LOSS_DIMS),
        )
        # Bridge B P1 guard: concept:http-request is the executing operation,
        # not a request-object constructor. Library callsites like libcurl
        # perform, Java HttpClient send, and Python urllib.request.urlopen all
        # produce HttpResponse data; if the concept ever regresses to returning
        # HttpRequest, Bridges C and D cannot honestly tag those callsites.
        self.assertEqual(request["return_sort"]["name"], "HttpResponse")
        self.assertEqual(request["post"]["result"], "HttpResponse")
        # The NetworkRequest effect must travel with this concept; without it
        # the substrate has no way to flag the operation as effectful.
        self.assertIn(
            {"kind": "effect-signature", "name": "NetworkRequest"},
            request["effects"]["effects"],
        )

        allowed_methods = request["pre"]["args"][1]["args"]
        self.assertEqual(
            [item["value"] for item in allowed_methods],
            ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"],
        )

        self.assertEqual(response["fn_name"], "concept:http-response")
        self.assertEqual(response["formals"], ["status", "headers", "body"])
        self.assertEqual(response["post"]["operator"], "http-response")
        self.assertEqual(
            sorted(response["post"]["loss_record"]),
            sorted(http.HTTP_RESPONSE_LOSS_DIMS),
        )

    def test_supporting_shapes_are_minimal_operation_contract_specs(self):
        specs = http.build_shape_specs()
        for slug in ["url", "header-map", "byte-stream"]:
            spec = specs[slug]
            self.assertEqual(spec["kind"], "algorithm")
            self.assertEqual(spec["fn_name"], f"concept:{slug}")
            self.assertEqual(spec["pre"], {"args": [], "kind": "atomic", "name": "true"})
            self.assertEqual(spec["effects"], {"effects": []})

    def test_specs_do_not_add_top_level_schema_fields(self):
        allowed = {
            "effects",
            "fn_name",
            "formal_sorts",
            "formals",
            "kind",
            "post",
            "pre",
            "return_sort",
        }
        for spec in http.build_shape_specs().values():
            self.assertEqual(set(spec), allowed)

    def test_examples_cover_requested_language_surfaces(self):
        examples = http.build_examples_md()
        self.assertIn("libcurl", examples)
        self.assertIn("java.net.http", examples)
        self.assertIn("urllib.request", examples)
        self.assertIn("concept:http-request", examples)
        self.assertIn("concept:http-response", examples)

    def test_generated_payloads_are_ascii_and_avoid_unicode_dash_chars(self):
        payload = json.dumps(http.build_shape_specs(), ensure_ascii=False) + http.build_examples_md()
        self.assertNotIn("\u2013", payload)
        self.assertNotIn("\u2014", payload)
        payload.encode("ascii")


if __name__ == "__main__":
    unittest.main()
