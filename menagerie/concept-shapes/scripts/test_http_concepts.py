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
        # loss_dimensions lives at the top level of the algorithm shape as
        # catalog metadata (the named axes along which realizations may
        # diverge), sorted for byte stability. Per-realization values live
        # on RealizationDesugaringMemento / LossyMorphismMemento; the shape
        # carries only the names.
        self.assertEqual(
            request["loss_dimensions"],
            sorted(http.HTTP_REQUEST_LOSS_DIMS),
        )
        # Bridge B Opus-review N1 guard: loss_record must NOT be nested inside
        # post. That placement is schema-novel and no Rust reader consumes it.
        # Loss values belong on LossyMorphismMemento (Rust LossRecord at
        # provekit-ir-types/src/lib.rs L508), not on the abstract shape.
        self.assertNotIn("loss_record", request["post"])
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
            response["loss_dimensions"],
            sorted(http.HTTP_RESPONSE_LOSS_DIMS),
        )
        self.assertNotIn("loss_record", response["post"])

    def test_supporting_shapes_are_minimal_operation_contract_specs(self):
        specs = http.build_shape_specs()
        for slug in ["url", "header-map", "byte-stream"]:
            spec = specs[slug]
            self.assertEqual(spec["kind"], "algorithm")
            self.assertEqual(spec["fn_name"], f"concept:{slug}")
            self.assertEqual(spec["pre"], {"args": [], "kind": "atomic", "name": "true"})
            self.assertEqual(spec["effects"], {"effects": []})

    def test_specs_do_not_add_top_level_schema_fields_beyond_loss_dimensions(self):
        """Top-level fields are the canonical algorithm-shape set plus
        the new optional `loss_dimensions` catalog-metadata field.

        `loss_dimensions: List[String]` carries only the named axes along
        which realizations may diverge; concrete per-realization values
        live on RealizationDesugaringMemento / LossyMorphismMemento (Rust
        type LossRecord at provekit-ir-types/src/lib.rs L508). Shapes
        without divergence dimensions (url, header-map, byte-stream) omit
        the field; algorithm shapes that document divergence (http-request,
        http-response) include it.
        """
        core = {
            "effects",
            "fn_name",
            "formal_sorts",
            "formals",
            "kind",
            "post",
            "pre",
            "return_sort",
        }
        optional = {"loss_dimensions"}
        for slug, spec in http.build_shape_specs().items():
            extra = set(spec) - core
            self.assertTrue(
                extra.issubset(optional),
                f"{slug} carries unknown top-level fields {extra - optional}",
            )
            self.assertTrue(core.issubset(set(spec)), f"{slug} is missing core fields {core - set(spec)}")

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
