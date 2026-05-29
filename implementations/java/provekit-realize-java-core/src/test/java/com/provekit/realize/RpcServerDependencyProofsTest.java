// SPDX-License-Identifier: Apache-2.0

package com.provekit.realize;

import com.provekit.ir.Jcs;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.ByteArrayOutputStream;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.util.HashSet;
import java.util.Set;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

import static org.junit.jupiter.api.Assertions.*;

public class RpcServerDependencyProofsTest {
    private static final String PROOF_A =
        "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.proof";
    private static final String PROOF_B =
        "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.proof";

    @TempDir
    Path tempDir;

    @Test
    public void resolveDependencyProofsReturnsProofsFromMavenClasspathDependencies() throws Exception {
        Path depOne = writeJar("dep-one.jar", "META-INF/provekit/" + PROOF_A, "proof-one");
        Path depTwo = writeJar("dep-two.jar", "nested/proofs/" + PROOF_B, "proof-two");
        Path project = writeMavenProject("""
            <dependency>
              <groupId>fixture</groupId>
              <artifactId>dep-one</artifactId>
              <version>1.0.0</version>
              <scope>system</scope>
              <systemPath>%s</systemPath>
            </dependency>
            <dependency>
              <groupId>fixture</groupId>
              <artifactId>dep-two</artifactId>
              <version>1.0.0</version>
              <scope>system</scope>
              <systemPath>%s</systemPath>
            </dependency>
            """.formatted(depOne, depTwo));

        String response = invokeHandle(resolveRequest(project));

        Jcs.Obj doc = (Jcs.Obj) Jcs.parse(response);
        assertNull(doc.get("error"), "resolve_dependency_proofs must not error: " + response);
        Jcs.Arr proofs = doc.objectField("result").arrayField("proofs");
        assertEquals(2, proofs.values().size(), "expected both dependency proofs: " + response);

        Set<String> cids = new HashSet<>();
        Set<String> contents = new HashSet<>();
        for (Jcs.Json item : proofs.values()) {
            Jcs.Obj proof = assertInstanceOf(Jcs.Obj.class, item, "proof must be a JSON object");
            cids.add(proof.stringField("cid"));
            contents.add(new String(
                Base64.getDecoder().decode(proof.stringField("bytes_base64")),
                StandardCharsets.UTF_8
            ));
            String source = proof.stringField("source");
            assertTrue(
                source.startsWith("java-jar:"),
                "source must be a diagnostic label, not a filesystem authority: " + source
            );
        }
        assertEquals(Set.of(stripProof(PROOF_A), stripProof(PROOF_B)), cids);
        assertEquals(Set.of("proof-one", "proof-two"), contents);
    }

    @Test
    public void resolveDependencyProofsReturnsEmptyArrayWhenMavenClasspathHasNoProofs() throws Exception {
        Path dep = writeJar("plain-dep.jar", "META-INF/plain.txt", "not a proof");
        Path project = writeMavenProject("""
            <dependency>
              <groupId>fixture</groupId>
              <artifactId>plain-dep</artifactId>
              <version>1.0.0</version>
              <scope>system</scope>
              <systemPath>%s</systemPath>
            </dependency>
            """.formatted(dep));

        String response = invokeHandle(resolveRequest(project));

        Jcs.Obj doc = (Jcs.Obj) Jcs.parse(response);
        assertNull(doc.get("error"), "resolve_dependency_proofs must not error: " + response);
        Jcs.Arr proofs = doc.objectField("result").arrayField("proofs");
        assertTrue(proofs.values().isEmpty(), "expected no dependency proofs: " + response);
    }

    private static String stripProof(String name) {
        return name.substring(0, name.length() - ".proof".length());
    }

    private Path writeJar(String name, String entryName, String content) throws Exception {
        Path jar = tempDir.resolve(name);
        try (JarOutputStream out = new JarOutputStream(Files.newOutputStream(jar))) {
            JarEntry entry = new JarEntry(entryName);
            out.putNextEntry(entry);
            out.write(content.getBytes(StandardCharsets.UTF_8));
            out.closeEntry();
        }
        return jar;
    }

    private Path writeMavenProject(String dependencies) throws Exception {
        Path project = tempDir.resolve("project-" + Math.abs(dependencies.hashCode()));
        Files.createDirectories(project);
        Files.writeString(project.resolve("pom.xml"), """
            <project xmlns="http://maven.apache.org/POM/4.0.0"
                     xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                     xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
              <modelVersion>4.0.0</modelVersion>
              <groupId>fixture</groupId>
              <artifactId>user-project</artifactId>
              <version>1.0.0</version>
              <dependencies>
            %s
              </dependencies>
            </project>
            """.formatted(dependencies), StandardCharsets.UTF_8);
        return project;
    }

    private String resolveRequest(Path project) {
        return "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"provekit.plugin.resolve_dependency_proofs\",\"params\":{\"project_root\":"
            + JsonUtil.quoted(project.toString())
            + "}}";
    }

    private String invokeHandle(String jsonLine) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        PrintWriter writer = new PrintWriter(new OutputStreamWriter(bytes, StandardCharsets.UTF_8), true);
        RpcServer server = new RpcServer();

        Field outField = RpcServer.class.getDeclaredField("out");
        outField.setAccessible(true);
        outField.set(server, writer);

        Method handle = RpcServer.class.getDeclaredMethod("handle", String.class);
        handle.setAccessible(true);
        handle.invoke(server, jsonLine);

        writer.flush();
        return bytes.toString(StandardCharsets.UTF_8).trim();
    }
}
