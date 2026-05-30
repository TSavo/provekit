// SPDX-License-Identifier: Apache-2.0

package provekit.lift.scala_source

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path}
import scala.jdk.CollectionConverters.*

final class ScalaSourceLifterTest extends munit.FunSuite:
  test("sugar body emits ast_template alongside body_text"):
    val body = mustSugarBodySource(
      """package shim
        |
        |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http", family = "concept:family:http")
        |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
        |""".stripMargin,
      "Fetch",
    )

    assertEquals(body("body_text").str, "http.get(url, headers)")
    assertEquals(body("source_cid").str, ScalaTemplate.cidOfUtf8("http.get(url, headers)"))
    assertEquals(jsonStrings(body("param_names")), Seq("url", "headers"))

    val template = body("ast_template").obj
    assertEquals(template("kind").str, "block")
    val stmt = template("stmts").arr.head.obj
    assertEquals(stmt("kind").str, "expr_stmt")
    val call = stmt("expr").obj
    assertEquals(call("kind").str, "method_call")
    assertEquals(call("method").str, "get")
    assertEquals(call("receiver").obj("name").str, "http")
    val args = call("args").arr.map(_.obj).toSeq
    assertEquals(args(0)("kind").str, "param_ref")
    assertEquals(args(0)("index").num.toInt, 1)
    assertEquals(args(1)("kind").str, "param_ref")
    assertEquals(args(1)("index").num.toInt, 2)
    assertEquals(body("template_cid").str, ScalaTemplate.templateCid(template))

  test("sugar body template_cid is stable under parameter renaming"):
    val bodyA = mustSugarBodySource(
      """package shim
        |
        |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http")
        |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
        |""".stripMargin,
      "Fetch",
    )
    val bodyB = mustSugarBodySource(
      """package shim
        |
        |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http")
        |def Fetch(addr: String, hdrs: Header): Response = http.get(addr, hdrs)
        |""".stripMargin,
      "Fetch",
    )

    assertEquals(bodyA("ast_template"), bodyB("ast_template"))
    assertEquals(bodyA("template_cid").str, bodyB("template_cid").str)
    assertNotEquals(bodyA("source_cid").str, bodyB("source_cid").str)

  test("recognize emits exact tag for alpha-equivalent user function"):
    val binding = mustBindingTemplate(
      concept = "concept:http-request",
      library = "provekit-shim-scala-http",
      family = "concept:family:http",
      source =
        """package shim
          |
          |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http", family = "concept:family:http")
          |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
          |""".stripMargin,
      functionName = "Fetch",
    )
    val root = Files.createTempDirectory("provekit-scala-recognize-")
    val rel = Path.of("src", "main", "scala", "Fetch.scala")
    write(root.resolve(rel),
      """package app
        |
        |def FetchURL(u: String, h: Header): Response = http.get(u, h)
        |""".stripMargin,
    )

    val response = ScalaRecognizer.recognize(
      RecognizeParams(root.toString, Seq(rel.toString.replace('\\', '/')), Seq(binding)),
    )

    assertEquals(response.tags.length, 1)
    val tag = response.tags.head
    assertEquals(tag.file, rel.toString.replace('\\', '/'))
    assertEquals(tag.functionName, "FetchURL")
    assertEquals(tag.conceptName, "concept:http-request")
    assertEquals(tag.libraryTag, "provekit-shim-scala-http")
    assertEquals(tag.family, Some(ujson.Str("concept:family:http")))
    assertEquals(tag.templateCid, binding.templateCid)
    assertEquals(tag.contractCid, binding.contractCid)
    assertEquals(tag.matchTier, "exact")
    assertEquals(tag.paramBindings.map(_.sourceText), Seq("u", "h"))

  test("rpc recognize dispatch self-resolves exact tags from sugar templates"):
    val root = Files.createTempDirectory("provekit-scala-recognize-rpc-")
    val shim = Path.of("Shim.scala")
    write(root.resolve(shim),
      """package shim
        |
        |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http", family = "concept:family:http")
        |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
        |""".stripMargin,
    )
    val rel = Path.of("Fetch.scala")
    write(root.resolve(rel),
      """package app
        |
        |def FetchURL(u: String, h: Header): Response = http.get(u, h)
        |""".stripMargin,
    )

    val response = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 7,
        "method" -> "provekit.plugin.recognize",
        "params" -> ujson.Obj(
          "project_root" -> root.toString,
          "source_paths" -> ujson.Arr(shim.toString, rel.toString),
        ),
      ),
    )

    assert(!response.obj.contains("error"))
    assertEquals(response("id").num.toInt, 7)
    val tags = response("result")("tags").arr
    assertEquals(tags.length, 1)
    assertEquals(tags.head("function_name").str, "FetchURL")
    assertEquals(tags.head("match_tier").str, "exact")

  test("recognize returns empty tags for non-matching source"):
    val binding = mustBindingTemplate(
      concept = "concept:http-request",
      library = "provekit-shim-scala-http",
      family = "",
      source =
        """package shim
          |
          |@sugar.bind(concept = "concept:http-request", library = "provekit-shim-scala-http")
          |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
          |""".stripMargin,
      functionName = "Fetch",
    )
    val root = Files.createTempDirectory("provekit-scala-recognize-")
    val rel = Path.of("Fetch.scala")
    write(root.resolve(rel),
      """package app
        |
        |def FetchURL(u: String, h: Header): Response = completelyDifferent(u, h)
        |""".stripMargin,
    )

    val response = ScalaRecognizer.recognize(
      RecognizeParams(root.toString, Seq(rel.toString), Seq(binding)),
    )

    assert(response.tags.isEmpty)

  test("recognize routes multiple bindings per call-site pool"):
    val httpBinding = mustBindingTemplate(
      concept = "concept:http-request",
      library = "http-lib",
      family = "concept:family:http",
      source =
        """package shim
          |
          |@sugar.bind(concept = "concept:http-request", library = "http-lib", family = "concept:family:http")
          |def Fetch(url: String, headers: Header): Response = http.get(url, headers)
          |""".stripMargin,
      functionName = "Fetch",
    )
    val sqlBinding = mustBindingTemplate(
      concept = "concept:sql-execute",
      library = "sql-lib",
      family = "concept:family:sql",
      source =
        """package shim
          |
          |@sugar.bind(concept = "concept:sql-execute", library = "sql-lib", family = "concept:family:sql")
          |def Exec(conn: DB, sql: String, args: Args): Result = conn.execute(sql, args)
          |""".stripMargin,
      functionName = "Exec",
    )
    val root = Files.createTempDirectory("provekit-scala-recognize-")
    val rel = Path.of("Calls.scala")
    write(root.resolve(rel),
      """package app
        |
        |def FetchURL(u: String, h: Header): Response = http.get(u, h)
        |
        |def RunQuery(db: DB, query: String, params: Args): Result = db.execute(query, params)
        |""".stripMargin,
    )

    val response = ScalaRecognizer.recognize(
      RecognizeParams(root.toString, Seq(rel.toString), Seq(httpBinding, sqlBinding)),
    )

    assertEquals(response.tags.length, 2)
    val routed = response.tags.map(tag => tag.conceptName -> tag.functionName).toMap
    assertEquals(routed("concept:http-request"), "FetchURL")
    assertEquals(routed("concept:sql-execute"), "RunQuery")
    assert(response.tags.forall(_.matchTier == "exact"))

  private def mustSugarBodySource(source: String, functionName: String): ujson.Obj =
    val result = ScalaSourceLifter.liftSource(source, "shim.scala", layer = "library-bindings")
    assertEquals(result.diagnostics, Seq.empty)
    val entry = result.ir.collectFirst {
      case obj: ujson.Obj if obj("source_function_name").str == functionName => obj
    }.getOrElse(fail(s"missing binding for $functionName in ${result.ir}"))
    entry("body_source").obj

  private def mustBindingTemplate(
      concept: String,
      library: String,
      family: String,
      source: String,
      functionName: String,
  ): BindingTemplate =
    val result = ScalaSourceLifter.liftSource(source, "shim.scala", layer = "library-bindings")
    val entry = result.ir.collectFirst {
      case obj: ujson.Obj if obj("source_function_name").str == functionName => obj
    }.getOrElse(fail(s"missing binding for $functionName in ${result.ir}"))
    val body = entry("body_source").obj
    BindingTemplate(
      conceptName = concept,
      libraryTag = library,
      family = Option.when(family.nonEmpty)(ujson.Str(family)),
      astTemplate = body("ast_template"),
      templateCid = body("template_cid").str,
      paramNames = jsonStrings(body("param_names")),
      contractCid = "blake3-512:" + ("c" * 128),
    )

  private def jsonStrings(value: ujson.Value): Seq[String] =
    value.arr.map(_.str).toSeq

  private def bindingTemplateJson(binding: BindingTemplate): ujson.Obj =
    val obj = ujson.Obj(
      "concept_name" -> binding.conceptName,
      "library_tag" -> binding.libraryTag,
      "ast_template" -> binding.astTemplate,
      "template_cid" -> binding.templateCid,
      "param_names" -> ujson.Arr.from(binding.paramNames),
      "contract_cid" -> binding.contractCid,
    )
    binding.family.foreach(value => obj("family") = value)
    obj

  private def write(path: Path, contents: String): Unit =
    Files.createDirectories(path.getParent)
    Files.writeString(path, contents, StandardCharsets.UTF_8)
