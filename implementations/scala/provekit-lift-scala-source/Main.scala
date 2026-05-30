// SPDX-License-Identifier: Apache-2.0

package provekit.lift.scala_source

import java.io.{BufferedReader, InputStreamReader}
import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Path}

import org.bouncycastle.crypto.digests.Blake3Digest
import scala.collection.mutable
import scala.jdk.CollectionConverters.*
import scala.meta.*

object Main:
  def main(args: Array[String]): Unit =
    if args.contains("--rpc") then ScalaSourceRpc.run()
    else System.err.println("provekit-lift-scala-source expects --rpc")

final case class LiftResult(
    ir: Seq[ujson.Value],
    diagnostics: Seq[ujson.Value],
    callEdges: Seq[CallEdgeDecl] = Seq.empty,
)

final case class SourceSpan(
    startLine: Int,
    startCol: Int,
    endLine: Int,
    endCol: Int,
):
  def toJson: ujson.Obj =
    ujson.Obj(
      "start_line" -> startLine,
      "start_col" -> startCol,
      "end_line" -> endLine,
      "end_col" -> endCol,
    )

final case class BindingTemplate(
    conceptName: String,
    libraryTag: String,
    family: Option[ujson.Value],
    astTemplate: ujson.Value,
    templateCid: String,
    paramNames: Seq[String],
    contractCid: String,
)

final case class RecognizeParams(
    projectRoot: String,
    sourcePaths: Seq[String],
    bindingTemplates: Seq[BindingTemplate],
    templateSourceFiles: Set[String] = Set.empty,
)

final case class ParamBinding(index: Int, sourceText: String):
  def toJson: ujson.Obj =
    ujson.Obj("index" -> index, "source_text" -> sourceText)

final case class CallSiteLocus(file: String, line: Int, column: Int):
  def toJson: ujson.Obj =
    ujson.Obj(
      "column" -> column,
      "file" -> file,
      "line" -> line,
    )

final case class CallEdgeDecl(
    sourceContractCid: String,
    targetContractCid: Option[String],
    targetSymbol: String,
    callSiteLocus: CallSiteLocus,
):
  def toJson: ujson.Obj =
    ujson.Obj(
      "callSiteLocus" -> callSiteLocus.toJson,
      "evidenceTerm" -> ujson.Obj(
        "args" -> ujson.Arr(),
        "kind" -> "atomic",
        "name" -> "call-site-obligation",
      ),
      "kind" -> "call-edge",
      "schemaVersion" -> "1",
      "sourceContractCid" -> sourceContractCid,
      "targetContractCid" -> targetContractCid.map(ujson.Str(_)).getOrElse(ujson.Null),
      "targetSymbol" -> targetSymbol,
    )

final case class RecognizeTag(
    file: String,
    span: SourceSpan,
    functionName: String,
    conceptName: String,
    libraryTag: String,
    family: Option[ujson.Value],
    templateCid: String,
    contractCid: String,
    matchTier: String,
    paramBindings: Seq[ParamBinding],
):
  def toJson: ujson.Obj =
    val obj = ujson.Obj(
      "file" -> file,
      "span" -> span.toJson,
      "function_name" -> functionName,
      "concept_name" -> conceptName,
      "library_tag" -> libraryTag,
      "template_cid" -> templateCid,
      "contract_cid" -> contractCid,
      "match_tier" -> matchTier,
      "param_bindings" -> ujson.Arr.from(paramBindings.map(_.toJson)),
    )
    family.foreach(value => obj("family") = value)
    obj

final case class RecognizeResponse(tags: Seq[RecognizeTag]):
  def toJson: ujson.Obj =
    ujson.Obj("tags" -> ujson.Arr.from(tags.map(_.toJson)))

private final case class ParsedSource(tree: Source, diagnostics: Seq[ujson.Value])

private final case class SugarBinding(
    conceptName: String,
    targetLibraryTag: String,
    family: Option[String],
    libraryVersion: Option[String],
    observedDimension: Option[String],
    loss: Seq[String],
)

object ScalaSourceLifter:
  def liftSource(source: String, sourcePath: String, layer: String = "all"): LiftResult =
    parseSource(source, sourcePath) match
      case Left(diagnostic) => LiftResult(Seq.empty, Seq(diagnostic))
      case Right(parsed) =>
        val emitBindings = layer == "library-bindings" || layer == "all"
        val ir =
          if emitBindings then
            ScalaTrees.definitions(parsed.tree).flatMap(defn => libraryBindingEntry(defn, sourcePath))
          else Seq.empty
        val callEdges =
          if layer == "all" then ScalaCallEdges.resolve(parsed.tree, sourcePath)
          else Seq.empty
        LiftResult(ir, parsed.diagnostics, callEdges)

  def liftPaths(workspaceRoot: String, sourcePaths: Seq[String], layer: String = "all"): LiftResult =
    val root = Path.of(if workspaceRoot.nonEmpty then workspaceRoot else ".").toAbsolutePath.normalize()
    val requested = if sourcePaths.nonEmpty then sourcePaths else Seq(".")
    val ir = mutable.ArrayBuffer.empty[ujson.Value]
    val diagnostics = mutable.ArrayBuffer.empty[ujson.Value]
    val callEdges = mutable.ArrayBuffer.empty[CallEdgeDecl]
    for path <- expandScalaPaths(root, requested) do
      val relPath =
        try root.relativize(path).toString.replace('\\', '/')
        catch case _: IllegalArgumentException => path.toString
      try
        val result = liftSource(Files.readString(path, StandardCharsets.UTF_8), relPath, layer)
        ir ++= result.ir
        diagnostics ++= result.diagnostics
        callEdges ++= result.callEdges
      catch
        case e: Exception =>
          diagnostics += ujson.Obj(
            "kind" -> "io-error",
            "message" -> s"cannot read '$relPath': ${e.getMessage}",
          )
    LiftResult(ir.toSeq, diagnostics.toSeq, callEdges.toSeq)

  def parseForLsp(source: String, sourcePath: String): ujson.Obj =
    parseSource(source, sourcePath) match
      case Left(diagnostic) =>
        ujson.Obj(
          "declarations" -> ujson.Arr(),
          "callEdges" -> ujson.Arr(),
          "warnings" -> ujson.Arr(diagnostic),
        )
      case Right(parsed) =>
        val seen = mutable.LinkedHashSet.empty[String]
        val declarations = ScalaTrees.definitions(parsed.tree).flatMap { defn =>
          val name = defn.name.value
          Option.when(name.nonEmpty && seen.add(name)) {
            ujson.Obj(
              "kind" -> "contract",
              "name" -> name,
              "outBinding" -> "out",
            )
          }
        }
        ujson.Obj(
          "declarations" -> ujson.Arr.from(declarations),
          "callEdges" -> ujson.Arr.from(ScalaCallEdges.resolve(parsed.tree, sourcePath).map(_.toJson)),
          "warnings" -> ujson.Arr.from(parsed.diagnostics),
        )

  private def libraryBindingEntry(defn: Defn.Def, sourcePath: String): Option[ujson.Obj] =
    sugarBinding(defn).map { binding =>
      val paramNames = ScalaTemplate.functionParamNames(defn)
      val paramTypes = defn.paramss.flatten.map(_.decltpe.map(_.syntax).getOrElse(""))
      val returnType = defn.decltpe.map(_.syntax).getOrElse("")
      val signatureShape = ujson.Obj(
        "param_names" -> ujson.Arr.from(paramNames),
        "param_types" -> ujson.Arr.from(paramTypes),
        "return_type" -> returnType,
      )
      val termShape = ScalaTemplate.functionBodyTemplate(defn)
      val bodyText = ScalaTemplate.bodyText(defn)
      val bodySource = ujson.Obj(
        "file" -> sourcePath,
        "source_cid" -> ScalaTemplate.cidOfUtf8(bodyText),
        "span" -> ScalaTrees.span(defn).toJson,
        "body_text" -> bodyText,
        "ast_template" -> termShape,
        "template_cid" -> ScalaTemplate.templateCid(termShape),
        "param_names" -> ujson.Arr.from(paramNames),
      )
      val entry = ujson.Obj(
        "kind" -> "library-sugar-binding-entry",
        "body_source" -> bodySource,
        "concept_name" -> binding.conceptName,
        "loss_record_contribution" -> ujson.Obj(
          "form" -> "literal",
          "value" -> ujson.Obj("entries" -> ujson.Arr.from(binding.loss)),
        ),
        "param_names" -> ujson.Arr.from(paramNames),
        "param_types" -> ujson.Arr.from(paramTypes),
        "return_type" -> returnType,
        "signature_shape_cid" -> ScalaTemplate.templateCid(signatureShape),
        "source_function_name" -> defn.name.value,
        "target_language" -> "scala",
        "target_library_tag" -> binding.targetLibraryTag,
        "term_shape" -> termShape,
        "term_shape_cid" -> ScalaTemplate.templateCid(termShape),
      )
      binding.family.foreach(value => entry("family") = value)
      binding.libraryVersion.foreach(value => entry("library_version") = value)
      binding.observedDimension.foreach(value => entry("observed_dimension") = value)
      entry
    }

  private def sugarBinding(defn: Defn.Def): Option[SugarBinding] =
    defn.mods.collectFirst {
      case Mod.Annot(init) if isSugarBind(init) =>
        val args = namedStringArgs(init)
        val concept = args.getOrElse("concept", "")
        val library = args.getOrElse("library", "")
        if concept.nonEmpty && library.nonEmpty then
          SugarBinding(
            conceptName = concept,
            targetLibraryTag = library,
            family = args.get("family"),
            libraryVersion = args.get("version"),
            observedDimension = args.get("observed_dimension"),
            loss = stringListArg(init, "loss").getOrElse(Seq.empty),
          )
        else null
    }.filter(_ != null)

  private def isSugarBind(init: Init): Boolean =
    init.tpe.syntax == "sugar.bind" || init.tpe.syntax == "provekit.sugar.bind"

  private def namedStringArgs(init: Init): Map[String, String] =
    init.argss.flatMap(_.values).collect {
      case Term.Assign(Term.Name(name), Lit.String(value)) => name -> value
    }.toMap

  private def stringListArg(init: Init, name: String): Option[Seq[String]] =
    init.argss.flatMap(_.values).collectFirst {
      case Term.Assign(Term.Name(`name`), Term.Apply(Term.Name("List"), values)) =>
        values.collect { case Lit.String(value) => value }
      case Term.Assign(Term.Name(`name`), Term.Apply(Term.Name("Seq"), values)) =>
        values.collect { case Lit.String(value) => value }
    }

  private def parseSource(source: String, sourcePath: String): Either[ujson.Value, ParsedSource] =
    dialects.Scala3(Input.VirtualFile(sourcePath, source)).parse[Source] match
      case Parsed.Success(tree) => Right(ParsedSource(tree, Seq.empty))
      case Parsed.Error(pos, message, _) =>
        Left(
          ujson.Obj(
            "kind" -> "parse-error",
            "message" -> message,
            "path" -> sourcePath,
            "line" -> (pos.startLine + 1),
          ),
        )

  private def expandScalaPaths(root: Path, requested: Seq[String]): Seq[Path] =
    val seen = mutable.LinkedHashSet.empty[Path]
    for item <- requested do
      val path = if Path.of(item).isAbsolute then Path.of(item) else root.resolve(item)
      if Files.isDirectory(path) then
        Files.walk(path).iterator().asScala
          .filter(path => Files.isRegularFile(path) && path.toString.endsWith(".scala"))
          .foreach(path => seen += path.toAbsolutePath.normalize())
      else if Files.isRegularFile(path) && path.toString.endsWith(".scala") then
        seen += path.toAbsolutePath.normalize()
    seen.toSeq

object ScalaRecognizer:
  def recognize(params: RecognizeParams): RecognizeResponse =
    if params.projectRoot.isEmpty then throw IllegalArgumentException("missing `project_root`")
    val root = Path.of(params.projectRoot).toAbsolutePath.normalize()
    val bindingsByCid = params.bindingTemplates
      .filter(_.templateCid.nonEmpty)
      .map(binding => binding.templateCid -> binding)
      .toMap
    val tags = mutable.ArrayBuffer.empty[RecognizeTag]
    for path <- expandScalaPaths(root, params.sourcePaths) do
      val relPath =
        try root.relativize(path).toString.replace('\\', '/')
        catch case _: IllegalArgumentException => path.toString
      if !params.templateSourceFiles.contains(relPath) then
        val source =
          try Some(Files.readString(path, StandardCharsets.UTF_8))
          catch case _: Exception => None
        source.foreach { text =>
          dialects.Scala3(Input.VirtualFile(relPath, text)).parse[Source] match
            case Parsed.Success(tree) =>
              for defn <- ScalaTrees.definitions(tree) do
                recognizeDef(relPath, defn, bindingsByCid).foreach(tags += _)
            case _: Parsed.Error => ()
        }
    RecognizeResponse(tags.toSeq)

  private def recognizeDef(
      relPath: String,
      defn: Defn.Def,
      bindingsByCid: Map[String, BindingTemplate],
  ): Option[RecognizeTag] =
    val template = ScalaTemplate.functionBodyTemplate(defn)
    val cid = ScalaTemplate.templateCid(template)
    bindingsByCid.get(cid).map { binding =>
      val paramNames = ScalaTemplate.functionParamNames(defn)
      RecognizeTag(
        file = relPath,
        span = ScalaTrees.span(defn),
        functionName = defn.name.value,
        conceptName = binding.conceptName,
        libraryTag = binding.libraryTag,
        family = binding.family,
        templateCid = cid,
        contractCid = binding.contractCid,
        matchTier = "exact",
        paramBindings = paramNames.zipWithIndex.map { case (name, index) =>
          ParamBinding(index + 1, name)
        },
      )
    }

  private def expandScalaPaths(root: Path, requested: Seq[String]): Seq[Path] =
    val seen = mutable.LinkedHashSet.empty[Path]
    for item <- requested do
      val path = if Path.of(item).isAbsolute then Path.of(item) else root.resolve(item)
      if Files.isDirectory(path) then
        Files.walk(path).iterator().asScala
          .filter(path => Files.isRegularFile(path) && path.toString.endsWith(".scala"))
          .foreach(path => seen += path.toAbsolutePath.normalize())
      else if Files.isRegularFile(path) && path.toString.endsWith(".scala") then
        seen += path.toAbsolutePath.normalize()
    seen.toSeq

object ScalaCallEdges:
  def resolve(tree: Source, sourcePath: String): Seq[CallEdgeDecl] =
    val definitions = ScalaTrees.definitions(tree)
    val declaredNames = definitions.map(_.name.value).filter(_.nonEmpty).toSet
    if declaredNames.isEmpty then return Seq.empty

    val edges = mutable.ArrayBuffer.empty[CallEdgeDecl]
    val seen = mutable.LinkedHashSet.empty[String]
    for caller <- definitions do
      val callerName = caller.name.value
      if callerName.nonEmpty then
        for call <- caller.body.collect { case apply: Term.Apply => apply } do
          callTargetName(call).foreach { targetName =>
            if targetName != callerName && declaredNames.contains(targetName) then
              val line = call.fun.pos.startLine + 1
              val column = call.fun.pos.startColumn
              val key = s"$callerName\u0000$targetName\u0000$line\u0000$column"
              if seen.add(key) then
                edges += CallEdgeDecl(
                  sourceContractCid = s"pending-scala:$callerName",
                  targetContractCid = None,
                  targetSymbol = s"scala-kit:$targetName",
                  callSiteLocus = CallSiteLocus(sourcePath, line, column),
                )
          }
    edges.toSeq

  private def callTargetName(call: Term.Apply): Option[String] =
    call.fun match
      case Term.Name(name) => Some(name)
      case Term.Select(_, Term.Name(name)) => Some(name)
      case _ => None

object ScalaTemplate:
  def functionParamNames(defn: Defn.Def): Seq[String] =
    defn.paramss.flatten.map(_.name.value)

  def bodyText(defn: Defn.Def): String =
    defn.body match
      case Term.Block(stats) => stats.map(_.syntax).mkString("\n").trim
      case other => other.syntax.trim

  def functionBodyTemplate(defn: Defn.Def): ujson.Obj =
    blockTemplate(defn.body, functionParamNames(defn))

  def blockTemplate(body: Term, params: Seq[String]): ujson.Obj =
    val statements = body match
      case Term.Block(stats) => stats
      case other => Seq(other)
    ujson.Obj(
      "kind" -> "block",
      "stmts" -> ujson.Arr.from(statements.map(stmtTemplate(_, params))),
    )

  def stmtTemplate(stat: Stat, params: Seq[String]): ujson.Obj =
    stat match
      case Defn.Val(_, pats, _, rhs) =>
        ujson.Obj(
          "kind" -> "let",
          "pat" -> pats.headOption.map(patTemplate(_, params)).getOrElse(ujson.Obj("kind" -> "pat_other")),
          "init" -> exprTemplate(rhs, params),
        )
      case term: Term =>
        ujson.Obj(
          "kind" -> "expr_stmt",
          "expr" -> exprTemplate(term, params),
          "trailing_semi" -> false,
        )
      case other =>
        ujson.Obj("kind" -> "other", "variant" -> other.productPrefix)

  def exprTemplate(term: Term, params: Seq[String]): ujson.Value =
    term match
      case Term.Apply(Term.Select(receiver, Term.Name(method)), args) =>
        ujson.Obj(
          "kind" -> "method_call",
          "receiver" -> exprTemplate(receiver, params),
          "method" -> method,
          "args" -> ujson.Arr.from(args.map(exprTemplate(_, params))),
        )
      case Term.Apply(func, args) =>
        ujson.Obj(
          "kind" -> "call",
          "func" -> exprTemplate(func, params),
          "args" -> ujson.Arr.from(args.map(exprTemplate(_, params))),
        )
      case Term.ApplyInfix(left, Term.Name(op), _, args) =>
        ujson.Obj(
          "kind" -> "binary",
          "op" -> op,
          "left" -> exprTemplate(left, params),
          "right" -> args.headOption.map(exprTemplate(_, params)).getOrElse(ujson.Null),
        )
      case Term.ApplyUnary(Term.Name(op), arg) =>
        ujson.Obj("kind" -> "unary", "op" -> op, "expr" -> exprTemplate(arg, params))
      case Term.Return(expr) =>
        ujson.Obj("kind" -> "return", "expr" -> exprTemplate(expr, params))
      case Term.Name(name) if params.contains(name) =>
        ujson.Obj("kind" -> "param_ref", "index" -> (params.indexOf(name) + 1))
      case Term.Name(name) =>
        ujson.Obj("kind" -> "ident", "name" -> name)
      case select: Term.Select =>
        fieldTemplateIfParamRoot(select, params).getOrElse {
          selectSegments(select) match
            case Some(segments) => ujson.Obj("kind" -> "path", "segments" -> ujson.Arr.from(segments))
            case None =>
              ujson.Obj(
                "kind" -> "field",
                "base" -> exprTemplate(select.qual, params),
                "member" -> select.name.value,
              )
        }
      case Term.Tuple(values) =>
        ujson.Obj("kind" -> "tuple", "elems" -> ujson.Arr.from(values.map(exprTemplate(_, params))))
      case Lit.String(value) => lit("str", value)
      case Lit.Int(value) => lit("int", value)
      case Lit.Long(value) => lit("int", value)
      case Lit.Float(value) => lit("float", value)
      case Lit.Double(value) => lit("float", value)
      case Lit.Boolean(value) => lit("bool", value)
      case Lit.Unit() => ujson.Obj("kind" -> "lit", "ty" -> "unit", "value" -> ujson.Null)
      case Lit.Null() => ujson.Obj("kind" -> "lit", "ty" -> "null", "value" -> ujson.Null)
      case Term.Block(stats) =>
        ujson.Obj("kind" -> "block", "stmts" -> ujson.Arr.from(stats.map(stmtTemplate(_, params))))
      case other =>
        ujson.Obj("kind" -> "other", "variant" -> other.productPrefix)

  def patTemplate(pat: Pat, params: Seq[String]): ujson.Obj =
    pat match
      case Pat.Var(Term.Name(name)) if params.contains(name) =>
        ujson.Obj("kind" -> "param_ref", "index" -> (params.indexOf(name) + 1))
      case Pat.Var(Term.Name(name)) =>
        ujson.Obj("kind" -> "binding", "name" -> name)
      case Pat.Tuple(values) =>
        ujson.Obj("kind" -> "pat_tuple", "elems" -> ujson.Arr.from(values.map(patTemplate(_, params))))
      case _ =>
        ujson.Obj("kind" -> "pat_other")

  def templateCid(value: ujson.Value): String =
    cidOfBytes(ujson.write(value).getBytes(StandardCharsets.UTF_8))

  def cidOfUtf8(value: String): String =
    cidOfBytes(value.getBytes(StandardCharsets.UTF_8))

  def cidOfBytes(bytes: Array[Byte]): String =
    val digest = Blake3Digest(512)
    digest.update(bytes, 0, bytes.length)
    val out = Array.ofDim[Byte](64)
    digest.doFinal(out, 0, out.length)
    "blake3-512:" + out.map(byte => f"${byte & 0xff}%02x").mkString

  private def lit(ty: String, value: String): ujson.Obj =
    ujson.Obj("kind" -> "lit", "ty" -> ty, "value" -> value)

  private def lit(ty: String, value: Long): ujson.Obj =
    ujson.Obj("kind" -> "lit", "ty" -> ty, "value" -> value)

  private def lit(ty: String, value: Double): ujson.Obj =
    ujson.Obj("kind" -> "lit", "ty" -> ty, "value" -> value)

  private def lit(ty: String, value: Boolean): ujson.Obj =
    ujson.Obj("kind" -> "lit", "ty" -> ty, "value" -> value)

  private def fieldTemplateIfParamRoot(select: Term.Select, params: Seq[String]): Option[ujson.Obj] =
    val parts = mutable.ArrayBuffer.empty[String]
    var current: Term = select
    while current.isInstanceOf[Term.Select] do
      val s = current.asInstanceOf[Term.Select]
      parts += s.name.value
      current = s.qual
    current match
      case Term.Name(root) if params.contains(root) =>
        var result: ujson.Value = ujson.Obj("kind" -> "param_ref", "index" -> (params.indexOf(root) + 1))
        for member <- parts.reverse do
          result = ujson.Obj("kind" -> "field", "base" -> result, "member" -> member)
        Some(result.obj)
      case _ => None

  private def selectSegments(select: Term.Select): Option[Seq[String]] =
    val parts = mutable.ArrayBuffer.empty[String]
    var current: Term = select
    while current.isInstanceOf[Term.Select] do
      val s = current.asInstanceOf[Term.Select]
      parts += s.name.value
      current = s.qual
    current match
      case Term.Name(root) => Some((parts += root).reverse.toSeq)
      case _ => None

object ScalaTrees:
  def definitions(tree: Tree): Seq[Defn.Def] =
    tree.collect { case defn: Defn.Def => defn }

  def span(tree: Tree): SourceSpan =
    SourceSpan(
      startLine = tree.pos.startLine + 1,
      startCol = tree.pos.startColumn,
      endLine = tree.pos.endLine + 1,
      endCol = tree.pos.endColumn,
    )

object ScalaSourceRpc:
  def run(): Unit =
    val reader = BufferedReader(InputStreamReader(System.in, StandardCharsets.UTF_8))
    var keepGoing = true
    while keepGoing do
      val line = reader.readLine()
      if line == null then keepGoing = false
      else if line.trim.nonEmpty then
        val response =
          try
            val request = ujson.read(line)
            val method = stringAt(request, "method")
            if method == "shutdown" || method == "provekit.plugin.shutdown" then keepGoing = false
            dispatch(request)
          catch
            case e: Throwable =>
              ujson.Obj(
                "jsonrpc" -> "2.0",
                "id" -> ujson.Null,
                "error" -> ujson.Obj("code" -> -32603, "message" -> e.getMessage),
              )
        println(ujson.write(response))
        Console.out.flush()

  def dispatch(request: ujson.Value): ujson.Obj =
    val id = request.obj.getOrElse("id", ujson.Null)
    val method = stringAt(request, "method")
    val params = request.obj.getOrElse("params", ujson.Obj())
    try
      val result = method match
        case "initialize" | "provekit.plugin.describe" => describe()
        case "lift" => lift(params)
        case "parse" => parse(params)
        case "provekit.plugin.recognize" => recognize(params)
        case "shutdown" | "provekit.plugin.shutdown" => ujson.Null
        case other => throw RpcFailure(-32601, s"METHOD_NOT_FOUND: $other")
      ujson.Obj("jsonrpc" -> "2.0", "id" -> id, "result" -> result)
    catch
      case e: RpcFailure =>
        ujson.Obj(
          "jsonrpc" -> "2.0",
          "id" -> id,
          "error" -> ujson.Obj("code" -> e.code, "message" -> e.getMessage),
        )

  private def describe(): ujson.Obj =
    ujson.Obj(
      "name" -> "provekit-lift-scala-source",
      "version" -> "0.1.0",
      "protocol_version" -> "pep/1.7.0",
      "capabilities" -> ujson.Obj(
        "authoring_surfaces" -> ujson.Arr("scala-source"),
        "ir_version" -> "bind-ir/1.0.0",
        "emits_signed_mementos" -> false,
      ),
    )

  private def lift(params: ujson.Value): ujson.Obj =
    val sourcePaths = stringArray(params.obj.getOrElse("source_paths", ujson.Arr()))
    val options = params.obj.get("options").collect { case obj: ujson.Obj => obj }.getOrElse(ujson.Obj())
    val layer = stringAt(options, "layer") match
      case "" => "all"
      case value => value
    val result = ScalaSourceLifter.liftPaths(
      stringAt(params, "workspace_root"),
      sourcePaths,
      layer,
    )
    ujson.Obj(
      "kind" -> "ir-document",
      "ir" -> ujson.Arr.from(result.ir),
      "callEdges" -> ujson.Arr.from(result.callEdges.map(_.toJson)),
      "diagnostics" -> ujson.Arr.from(result.diagnostics),
    )

  private def parse(params: ujson.Value): ujson.Obj =
    val language = stringAt(params, "language")
    if language.nonEmpty && language != "scala" then
      throw RpcFailure(-32602, s"language '$language' not supported by this plugin")
    ScalaSourceLifter.parseForLsp(
      rawStringAt(params, "source"),
      stringAt(params, "path"),
    )

  private def recognize(params: ujson.Value): ujson.Obj =
    ScalaRecognizer.recognize(parseRecognizeParams(params)).toJson

  private def parseRecognizeParams(params: ujson.Value): RecognizeParams =
    val projectRoot = stringAt(params, "project_root")
    val sourcePaths = stringArray(params.obj.getOrElse("source_paths", ujson.Arr()))
    val suppliedTemplates = params.obj
      .get("binding_templates")
      .collect { case arr: ujson.Arr => arr.value.collect { case obj: ujson.Obj => parseBindingTemplate(obj) }.toSeq }
      .getOrElse(Seq.empty)
    val selfResolved = if suppliedTemplates.nonEmpty then Seq.empty else
      ScalaSourceLifter
        .liftPaths(projectRoot, sourcePaths, "library-bindings")
        .ir
        .collect { case obj: ujson.Obj => obj }
    val selfResolvedTemplates = selfResolved.map(parseSugarBindingTemplate)
    val templateFiles = selfResolved.flatMap { entry =>
      entry.obj
        .get("body_source")
        .collect { case body: ujson.Obj => stringAt(body, "file") }
        .filter(_.nonEmpty)
    }.toSet
    RecognizeParams(
      projectRoot = projectRoot,
      sourcePaths = sourcePaths,
      bindingTemplates = if suppliedTemplates.nonEmpty then suppliedTemplates else selfResolvedTemplates,
      templateSourceFiles = templateFiles,
    )

  private def parseBindingTemplate(obj: ujson.Obj): BindingTemplate =
    BindingTemplate(
      conceptName = stringAt(obj, "concept_name"),
      libraryTag = firstString(obj, "library_tag", "target_library_tag").getOrElse(""),
      family = obj.obj.get("family"),
      astTemplate = obj.obj.getOrElse("ast_template", ujson.Null),
      templateCid = stringAt(obj, "template_cid"),
      paramNames = stringArray(obj.obj.getOrElse("param_names", ujson.Arr())),
      contractCid = stringAt(obj, "contract_cid"),
    )

  private def parseSugarBindingTemplate(obj: ujson.Obj): BindingTemplate =
    val body = obj.obj.get("body_source").collect { case body: ujson.Obj => body }.getOrElse(ujson.Obj())
    BindingTemplate(
      conceptName = stringAt(obj, "concept_name"),
      libraryTag = firstString(obj, "library_tag", "target_library_tag").getOrElse(""),
      family = obj.obj.get("family"),
      astTemplate = body.obj.getOrElse("ast_template", ujson.Null),
      templateCid = stringAt(body, "template_cid"),
      paramNames = stringArray(body.obj.getOrElse("param_names", ujson.Arr())),
      contractCid = stringAt(obj, "contract_cid"),
    )

  private def stringArray(value: ujson.Value): Seq[String] =
    value match
      case arr: ujson.Arr => arr.value.collect { case ujson.Str(text) => text }.toSeq
      case _ => Seq.empty

private final case class RpcFailure(code: Int, message: String) extends RuntimeException(message)

private def stringAt(value: ujson.Value, field: String): String =
  firstString(value, field).getOrElse("")

private def firstString(value: ujson.Value, fields: String*): Option[String] =
  value match
    case obj: ujson.Obj =>
      fields.view.flatMap(field => obj.obj.get(field).collect {
        case ujson.Str(text) if text.trim.nonEmpty => text.trim
      }).headOption
    case _ => None

private def rawStringAt(value: ujson.Value, field: String): String =
  value match
    case obj: ujson.Obj =>
      obj.obj.get(field).collect { case ujson.Str(text) => text }.getOrElse("")
    case _ => ""
