// SPDX-License-Identifier: Apache-2.0
//> using scala "3.8.3"
//> using dep com.lihaoyi::upickle:4.4.3
//> using dep org.bouncycastle:bcprov-jdk18on:1.84

package provekit.emit.scalatest

import java.io.{BufferedReader, InputStreamReader}
import java.nio.charset.StandardCharsets
import scala.concurrent.{Await, Future}
import scala.concurrent.ExecutionContext.Implicits.global
import scala.concurrent.duration.Duration

import org.bouncycastle.crypto.digests.Blake3Digest

object Main:
  def main(args: Array[String]): Unit =
    if args.contains("--rpc") then RpcServer.run()
    else System.err.println("provekit-emit-scala-scalatest expects --rpc")

object RpcServer:
  def run(): Unit =
    val reader = BufferedReader(InputStreamReader(System.in, StandardCharsets.UTF_8))
    var keepGoing = true
    while keepGoing do
      val line = reader.readLine()
      if line == null then keepGoing = false
      else if line.trim.nonEmpty then
        var responseId: ujson.Value = ujson.Null
        val response =
          try
            val request = ujson.read(line)
            val method = stringAt(request, "method")
            val id = request.obj.getOrElse("id", ujson.Null)
            responseId = id
            val params = request.obj.getOrElse("params", ujson.Obj())
            val result =
              method match
                case "provekit.plugin.describe" => describe()
                case "provekit.plugin.invoke" => ScalaTestEmitter.emit(params)
                case "provekit.plugin.check" => ScalaTestChecker.check(params)
                case "provekit.plugin.shutdown" =>
                  keepGoing = false
                  ujson.Null
                case other => throw RpcError(-32601, s"METHOD_NOT_FOUND: $other")
            ujson.Obj("jsonrpc" -> "2.0", "id" -> id, "result" -> result)
          catch
            case e: RpcError =>
              ujson.Obj(
                "jsonrpc" -> "2.0",
                "id" -> responseId,
                "error" -> ujson.Obj("code" -> e.code, "message" -> e.getMessage),
              )
            case e: Throwable =>
              ujson.Obj(
                "jsonrpc" -> "2.0",
                "id" -> responseId,
                "error" -> ujson.Obj("code" -> -32700, "message" -> s"PARSE_ERROR: ${e.getMessage}"),
              )
        println(ujson.write(response))
        Console.out.flush()

  private def describe(): ujson.Value =
    ujson.Obj(
      "kind" -> "emit",
      "target_language" -> "scala",
      "target_framework" -> "scalatest",
      "protocol_versions" -> ujson.Arr("pep/1.7.0"),
      "predicates" -> ujson.Arr(
        "concept:eq",
        "concept:ne",
        "concept:lt",
        "concept:gt",
        "concept:le",
        "concept:ge",
        "concept:option-is-some",
        "concept:option-is-none",
        "concept:not-null",
        "concept:fallible-err",
      ),
    )

final case class RpcError(code: Int, message: String) extends RuntimeException(message)

object ScalaTestEmitter:
  def emit(plan: ujson.Value): ujson.Value =
    val functionName = firstString(plan, "function", "function_name", "functionName").getOrElse("contract")
    val predicates = arrayAt(plan, "predicates").collect { case obj: ujson.Obj => obj }
    val emitted = collection.mutable.ArrayBuffer.empty[String]
    val unsupported = collection.mutable.ArrayBuffer.empty[String]
    val tests = collection.mutable.ArrayBuffer.empty[String]

    for (predicate, index) <- predicates.zipWithIndex do
      val head = canonicalHead(predicateHead(predicate).getOrElse(""))
      renderAssertion(head, predicate) match
        case Some(assertion) =>
          emitted += head
          tests += renderTest(head, index, declarationsFor(head, predicate), assertion)
        case None =>
          unsupported += (if head.nonEmpty then head else "<malformed>")

    val source = renderSuite(suiteName(functionName), tests.toSeq)
    ujson.Obj(
      "kind" -> "scala-scalatest-test-emission",
      "source" -> source,
      "path" -> "src/test/scala/ProvekitEmittedSuite.scala",
      "extension" -> "scala",
      "emitted_artifact_cid" -> blake3Cid(source),
      "emitted_predicates" -> ujson.Arr.from(emitted),
      "unsupported_predicates" -> ujson.Arr.from(unsupported),
      "is_complete" -> (unsupported.isEmpty && emitted.nonEmpty),
    )

  private def renderSuite(name: String, tests: Seq[String]): String =
    val body =
      if tests.isEmpty then """  test("has no emitted predicates") {}"""
      else tests.mkString("\n\n")
    s"""//> using scala "3.8.3"
       |//> using dep org.scalatest::scalatest:3.2.20
       |
       |import org.scalatest.funsuite.AnyFunSuite
       |
       |final class $name extends AnyFunSuite {
       |$body
       |}
       |""".stripMargin

  private def renderTest(head: String, index: Int, declarations: Seq[String], assertion: String): String =
    val lines = collection.mutable.ArrayBuffer(s"""  test("verifies $head $index") {""")
    declarations.foreach(decl => lines += s"    $decl")
    assertion.linesIterator.foreach(line => lines += s"    $line")
    lines += "  }"
    lines.mkString("\n")

  private def renderAssertion(head: String, predicate: ujson.Obj): Option[String] =
    val args = argumentObjects(predicate)
    head match
      case "eq" => binary(args)((left, right) => s"assertResult($right)($left)")
      case "ne" => binary(args)((left, right) => s"assert($left != $right)")
      case "lt" => binary(args)((left, right) => s"assert($left < $right)")
      case "gt" => binary(args)((left, right) => s"assert($left > $right)")
      case "le" => binary(args)((left, right) => s"assert($left <= $right)")
      case "ge" => binary(args)((left, right) => s"assert($left >= $right)")
      case "option-is-some" =>
        unary(args)(value => s"assert($value.nonEmpty)")
      case "option-is-none" =>
        unary(args)(value => s"assert($value.isEmpty)")
      case "not-null" =>
        unary(args)(value => s"assert($value != null)")
      case "fallible-err" =>
        if args.length != 1 then None
        else renderFallibleTerm(args.head).map(expr => s"intercept[Throwable] {\n      $expr\n    }")
      case _ => None

  private def binary(args: Seq[ujson.Obj])(f: (String, String) => String): Option[String] =
    if args.length != 2 then None
    else
      for
        left <- renderTerm(args(0))
        right <- renderTerm(args(1))
      yield f(left, right)

  private def unary(args: Seq[ujson.Obj])(f: String => String): Option[String] =
    if args.length != 1 then None
    else renderTerm(args.head).map(f)

  private def renderTerm(term: ujson.Obj): Option[String] =
    stringAt(term, "kind") match
      case "var" => stringOpt(term, "name").map(name => sanitizeIdentifier(name, "value"))
      case "const" => Some(renderConst(term.obj.getOrElse("value", ujson.Null)))
      case "op" | "ctor" => renderApplication(term)
      case _ => None

  private def renderFallibleTerm(term: ujson.Obj): Option[String] =
    stringAt(term, "kind") match
      case "var" => stringOpt(term, "name").map(name => s"${sanitizeIdentifier(name, "thunk")}()")
      case _ => renderTerm(term)

  private def renderConst(value: ujson.Value): String =
    value match
      case ujson.Null => "null"
      case ujson.Bool(v) => if v then "true" else "false"
      case ujson.Num(v) if v.isWhole && v >= Long.MinValue && v <= Long.MaxValue => v.toLong.toString
      case ujson.Num(v) => v.toString
      case ujson.Str(v) => quoteScala(v)
      case _ => "null"

  private def renderApplication(term: ujson.Obj): Option[String] =
    val name = firstString(term, "name", "conceptName", "concept_name").getOrElse("")
    val args = argumentObjects(term).map(renderTerm)
    if args.exists(_.isEmpty) then None
    else
      val rendered = args.flatten
      val cleanName = name.stripPrefix("concept:")
      cleanName match
        case "+" | "-" | "*" | "/" | "%" if rendered.length == 2 =>
          Some(s"(${rendered(0)} $cleanName ${rendered(1)})")
        case "array" | "list" => Some(rendered.mkString("Seq(", ", ", ")"))
        case "tuple" => Some(rendered.mkString("(", ", ", ")"))
        case other if other.nonEmpty =>
          Some(s"${sanitizeIdentifier(other, "fn")}(${rendered.mkString(", ")})")
        case _ => None

  private def declarationsFor(head: String, predicate: ujson.Obj): Seq[String] =
    val names = collection.mutable.ArrayBuffer.empty[String]
    collectVars(predicate, names)
    names.zipWithIndex.map { case (raw, index) =>
      val ident = sanitizeIdentifier(raw, s"v$index")
      head match
        case "option-is-some" => s"val $ident: Option[Int] = Some(1)"
        case "option-is-none" => s"val $ident: Option[Int] = None"
        case "not-null" => s"val $ident: String = ${quoteScala("value")}"
        case "fallible-err" => s"""val $ident: () => Any = () => throw RuntimeException("contract error")"""
        case _ => s"val $ident = ${placeholderValue(head, index)}"
    }.toSeq

  private def collectVars(value: ujson.Value, out: collection.mutable.ArrayBuffer[String]): Unit =
    value match
      case obj: ujson.Obj =>
        if stringAt(obj, "kind") == "var" then
          stringOpt(obj, "name").foreach(name => if !out.contains(name) then out += name)
        obj.obj.get("args").foreach(collectVars(_, out))
      case arr: ujson.Arr => arr.value.foreach(collectVars(_, out))
      case _ =>

  private def placeholderValue(head: String, index: Int): String =
    head match
      case "lt" | "le" => if index == 0 then "0" else "1"
      case "gt" | "ge" => if index == 0 then "1" else "0"
      case "ne" => if index == 0 then "0" else "1"
      case _ => "0"

  private def suiteName(functionName: String): String =
    val base = functionName
      .split("[^0-9A-Za-z]+")
      .filter(_.nonEmpty)
      .map(part => s"${part.head.toUpper}${part.drop(1)}")
      .mkString
    sanitizeTypeName(if base.nonEmpty then s"${base}ContractSuite" else "ProvekitContractSuite")

object ScalaTestChecker:
  def check(params: ujson.Value): ujson.Value =
    val outDir = firstString(params, "out_dir", "outDir").getOrElse {
      throw RpcError(-32602, "INVALID_PARAMS: missing out_dir")
    }
    val command = Seq("scala-cli", "test", ".", "--server=false")
    val builder = ProcessBuilder(command*)
    builder.directory(java.io.File(outDir))
    val process = builder.start()
    val stdoutF = Future(process.getInputStream.readAllBytes())
    val stderrF = Future(process.getErrorStream.readAllBytes())
    val exitCode = process.waitFor()
    val stdout = String(Await.result(stdoutF, Duration.Inf), StandardCharsets.UTF_8)
    val stderr = String(Await.result(stderrF, Duration.Inf), StandardCharsets.UTF_8)
    ujson.Obj(
      "ok" -> (exitCode == 0),
      "command" -> command.mkString(" "),
      "cwd" -> outDir,
      "stdout" -> stdout,
      "stderr" -> stderr,
      "exitCode" -> exitCode,
    )

def stringAt(value: ujson.Value, field: String): String =
  stringOpt(value, field).getOrElse("")

def stringOpt(value: ujson.Value, field: String): Option[String] =
  value match
    case obj: ujson.Obj =>
      obj.obj.get(field).collect { case ujson.Str(text) if text.trim.nonEmpty => text.trim }
    case _ => None

def firstString(value: ujson.Value, fields: String*): Option[String] =
  fields.view.flatMap(field => stringOpt(value, field)).headOption

def arrayAt(value: ujson.Value, field: String): Seq[ujson.Value] =
  value match
    case obj: ujson.Obj =>
      obj.obj.get(field).collect { case arr: ujson.Arr => arr.value.toSeq }.getOrElse(Seq.empty)
    case _ => Seq.empty

def argumentObjects(value: ujson.Obj): Seq[ujson.Obj] =
  value.obj.get("args").collect {
    case arr: ujson.Arr => arr.value.collect { case obj: ujson.Obj => obj }.toSeq
  }.getOrElse(Seq.empty)

def predicateHead(predicate: ujson.Obj): Option[String] =
  firstString(predicate, "name", "concept_name", "conceptName", "op", "head")

def canonicalHead(raw: String): String =
  raw.stripPrefix("concept:").replace("_", "-").toLowerCase match
    case "=" => "eq"
    case "!=" => "ne"
    case "<" => "lt"
    case ">" => "gt"
    case "<=" => "le"
    case ">=" => "ge"
    case "neq" => "ne"
    case "lte" => "le"
    case "gte" => "ge"
    case other => other

def sanitizeIdentifier(raw: String, fallback: String): String =
  val cleaned = raw.replace("-", "_").map { ch =>
    if ch.isLetterOrDigit || ch == '_' then ch else '_'
  }.mkString.replaceAll("^[^A-Za-z_]+", "")
  if cleaned.matches("[A-Za-z_][0-9A-Za-z_]*") then cleaned else fallback

def sanitizeTypeName(raw: String): String =
  val cleaned = sanitizeIdentifier(raw, "ProvekitContractSuite")
  if cleaned.headOption.exists(_.isUpper) then cleaned else s"${cleaned.head.toUpper}${cleaned.drop(1)}"

def quoteScala(value: String): String =
  value.flatMap {
    case '"' => "\\\""
    case '\\' => "\\\\"
    case '\n' => "\\n"
    case '\r' => "\\r"
    case '\t' => "\\t"
    case ch => ch.toString
  }.prepended('"').appended('"')

def blake3Cid(source: String): String =
  val bytes = source.getBytes(StandardCharsets.UTF_8)
  val digest = Blake3Digest(512)
  digest.update(bytes, 0, bytes.length)
  val out = Array.ofDim[Byte](64)
  digest.doFinal(out, 0, out.length)
  "blake3-512:" + out.map(byte => f"${byte & 0xff}%02x").mkString
