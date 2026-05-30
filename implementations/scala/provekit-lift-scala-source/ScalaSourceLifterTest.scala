// SPDX-License-Identifier: Apache-2.0

package provekit.lift.scala_source

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path}
import scala.jdk.CollectionConverters.*

final class ScalaSourceLifterTest extends munit.FunSuite:
  test("rpc initialize reports shared lsp protocol"):
    val response = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 1,
        "method" -> "initialize",
        "params" -> ujson.Obj(),
      ),
    )

    assert(!response.obj.contains("error"), response.render())
    val result = response("result")
    assertEquals(result("name").str, "provekit-lift-scala-source")
    assertEquals(result("version").str, "0.1.0")
    assertEquals(result("protocol_version").str, "provekit-lsp-shared/1")
    assertEquals(result("kit_id").str, "scala")
    assert(result("protocol_catalog_cid").str.startsWith("blake3-512:"))
    assertEquals(result("capabilities")("source_surfaces").arr.map(_.str).toSeq, Seq("scala-source"))
    assert(result("capabilities")("diagnostic_codes").arr.exists(_.str == "provekit.lsp.implication_failed"))

  test("rpc analyzeDocument emits shared callsite diagnostic"):
    val source =
      """def checkPositive(x: Int): Boolean =
        |  if x <= 0 then false else true
        |
        |def callerSatisfiesPre(): Boolean =
        |  val result = checkPositive(5)
        |  result
        |
        |def callerViolatesPre(): Boolean =
        |  val result = checkPositive(-1)
        |  result
        |
        |def callerWithLoop(): Boolean =
        |  for i <- 0 until 10 do
        |    val result = checkPositive(i)
        |    if !result then false
        |  true
        |""".stripMargin

    val response = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 2,
        "method" -> "analyzeDocument",
        "params" -> ujson.Obj(
          "kit_id" -> "scala",
          "uri" -> "file:///project/FloorFixture.scala",
          "file" -> "FloorFixture.scala",
          "text" -> source,
          "document_version" -> 42,
          "workspace_root" -> "/project",
          "accepted_protocol_catalog_cids" -> ujson.Arr(),
          "policy_cids" -> ujson.Arr(),
        ),
      ),
    )

    assert(!response.obj.contains("error"), response.render())
    val result = response("result")
    assertEquals(result("kind").str, "lsp-document-analysis")
    assertEquals(result("schema_version").str, "1")
    assertEquals(result("kit_id").str, "scala")
    assertEquals(result("uri").str, "file:///project/FloorFixture.scala")
    assertEquals(result("file").str, "FloorFixture.scala")
    assert(result("document_cid").str.startsWith("blake3-512:"))

    val diagnostics = result("diagnostics").arr
    assertEquals(diagnostics.length, 1)
    val diagnostic = diagnostics.head
    assertEquals(diagnostic("code").str, "provekit.lsp.implication_failed")
    assertEquals(diagnostic("severity").str, "error")
    assertEquals(diagnostic("producer").str, "forward-propagation")
    assertEquals(diagnostic("kit_id").str, "scala")
    assertEquals(diagnostic("data")("callee").str, "checkPositive")
    assertEquals(diagnostic("data")("missing_conjuncts").arr.map(_.str).toSeq, Seq("x > 0"))
    assertEquals(diagnostic("range")("start_line").num.toInt, 9)
    assertEquals(diagnostic("range")("start_col").num.toInt, 15)

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

  test("rpc lift emits same-language call edge locus"):
    val root = Files.createTempDirectory("provekit-scala-lsp-calledges-")
    val rel = Path.of("Calls.scala")
    write(root.resolve(rel),
      """package app
        |
        |def addOne(x: Int): Int = x + 1
        |
        |def callAddOne(x: Int): Int = addOne(x)
        |""".stripMargin,
    )

    val response = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 9,
        "method" -> "lift",
        "params" -> ujson.Obj(
          "workspace_root" -> root.toString,
          "source_paths" -> ujson.Arr(rel.toString.replace('\\', '/')),
        ),
      ),
    )

    assert(!response.obj.contains("error"), response.render())
    val callEdges = response("result")("callEdges").arr
    assertEquals(callEdges.length, 1)
    val edge = callEdges.head
    assertEquals(edge("kind").str, "call-edge")
    assertEquals(edge("schemaVersion").str, "1")
    assertEquals(edge("sourceContractCid").str, "pending-scala:callAddOne")
    assert(edge("targetContractCid").isNull)
    assertEquals(edge("targetSymbol").str, "scala-kit:addOne")
    assertEquals(edge("evidenceTerm")("name").str, "call-site-obligation")
    assertEquals(edge("callSiteLocus")("file").str, rel.toString.replace('\\', '/'))
    assertEquals(edge("callSiteLocus")("line").num.toInt, 5)
    assert(edge("callSiteLocus")("column").num.toInt > 0)

  test("rpc parse returns declarations and call edges"):
    val source =
      """package app
        |
        |def addOne(x: Int): Int = x + 1
        |
        |def callAddOne(x: Int): Int = addOne(x)
        |""".stripMargin

    val response = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 10,
        "method" -> "parse",
        "params" -> ujson.Obj(
          "path" -> "Calls.scala",
          "source" -> source,
        ),
      ),
    )

    assert(!response.obj.contains("error"), response.render())
    val result = response("result")
    val declarations = result("declarations").arr
    assertEquals(declarations.map(_("name").str).toSet, Set("addOne", "callAddOne"))
    assert(declarations.forall(_("kind").str == "contract"))
    assert(declarations.forall(_("outBinding").str == "out"))

    val edge = result("callEdges").arr.head
    assertEquals(edge("sourceContractCid").str, "pending-scala:callAddOne")
    assertEquals(edge("targetSymbol").str, "scala-kit:addOne")
    assertEquals(edge("callSiteLocus")("file").str, "Calls.scala")

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

  test("verify layer lifts Scala function bodies to body-bearing function contracts"):
    val result = ScalaSourceLifter.liftSource(
      """package app
        |
        |def double(x: Int): Int = x * 2
        |""".stripMargin,
      "Double.scala",
      layer = "verify",
    )

    assertEquals(result.diagnostics, Seq.empty)
    assertEquals(result.ir.length, 1)
    val contract = result.ir.head.obj
    assertEquals(contract("kind").str, "function-contract")
    assertEquals(contract("fnName").str, "double")
    assertEquals(contract("bridgeSourceSymbol").str, "double")
    assertEquals(jsonStrings(contract("formals")), Seq("x"))
    assertEquals(contract("formalSorts")(0)("name").str, "Int")
    val post = contract("post").obj
    assertEquals(post("name").str, "=")
    assertEquals(post("args")(0)("name").str, "result")
    val body = post("args")(1).obj
    assertEquals(body("kind").str, "ctor")
    assertEquals(body("name").str, "*")
    assertEquals(body("args")(0)("name").str, "x")
    assertEquals(body("args")(1)("value").num.toInt, 2)

  test("tests layer lifts ScalaTest assert callsites to source contracts"):
    val result = ScalaSourceLifter.liftSource(
      """package app
        |
        |import org.scalatest.funsuite.AnyFunSuite
        |
        |final class DoubleSpec extends AnyFunSuite {
        |  test("double three is six") {
        |    assert(double(3) == 6)
        |  }
        |}
        |""".stripMargin,
      "DoubleSpec.scala",
      layer = "tests",
    )

    assertEquals(result.diagnostics, Seq.empty)
    assertEquals(result.ir.length, 1)
    val contract = result.ir.head.obj
    assertEquals(contract("kind").str, "contract")
    assertEquals(contract("name").str, "double three is six::0")
    val inv = contract("inv").obj
    assertEquals(inv("kind").str, "atomic")
    assertEquals(inv("name").str, "=")
    val call = inv("args")(0).obj
    assertEquals(call("kind").str, "ctor")
    assertEquals(call("name").str, "double")
    assertEquals(call("args")(0)("value").num.toInt, 3)
    assertEquals(inv("args")(1)("value").num.toInt, 6)

  test("rpc lift dispatch honors Scala parity layers through options"):
    val root = Files.createTempDirectory("provekit-scala-rpc-layers-")
    val sourceRel = Path.of("Double.scala")
    val testRel = Path.of("DoubleSpec.scala")
    write(root.resolve(sourceRel),
      """package app
        |
        |def double(x: Int): Int = x * 2
        |""".stripMargin,
    )
    write(root.resolve(testRel),
      """package app
        |
        |import org.scalatest.funsuite.AnyFunSuite
        |
        |final class DoubleSpec extends AnyFunSuite {
        |  test("double three is six") {
        |    assert(double(3) == 6)
        |  }
        |}
        |""".stripMargin,
    )

    val sourceResponse = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 11,
        "method" -> "lift",
        "params" -> ujson.Obj(
          "workspace_root" -> root.toString,
          "source_paths" -> ujson.Arr(sourceRel.toString),
          "options" -> ujson.Obj("layer" -> "verify"),
        ),
      ),
    )
    val testResponse = ScalaSourceRpc.dispatch(
      ujson.Obj(
        "jsonrpc" -> "2.0",
        "id" -> 12,
        "method" -> "lift",
        "params" -> ujson.Obj(
          "workspace_root" -> root.toString,
          "source_paths" -> ujson.Arr(testRel.toString),
          "options" -> ujson.Obj("layer" -> "tests"),
        ),
      ),
    )

    assert(!sourceResponse.obj.contains("error"))
    assert(!testResponse.obj.contains("error"))
    assertEquals(sourceResponse("result")("ir").arr.head("kind").str, "function-contract")
    assertEquals(testResponse("result")("ir").arr.head("kind").str, "contract")

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
