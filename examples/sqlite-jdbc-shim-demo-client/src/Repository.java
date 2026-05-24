// SPDX-License-Identifier: Apache-2.0
//
// sqlite-jdbc-shim-demo-client: a CONSUMER that needs SQL operations via the
// concept:family:sql CONTRACT, not a specific library. The carrier sites below
// cite a representative subset of the sqlite-jdbc shim's concepts (connection
// lifecycle, execute/query/prepare, transaction control, row reads, changes
// count, busy timeout). `provekit materialize --library sqlite-jdbc` realizes
// them against the org.xerial:sqlite-jdbc shim (provekit-shim-java-sqlite-jdbc).
//
// The signatures are library-NEUTRAL (java.sql.* surface types). The body
// between each carrier and the next site is what the kit's assemble RPC fills
// from the shim's signed .proof (NOT a disk JSON cache). The .proof was lifted
// from the correct @ProveKitSugar source, so the emitted bodies are correct
// even though the deleted java-canonical-bodies-sqlite-jdbc.json cache carried
// a pre-existing `${param}`-in-imports substitution bug.

package org.provekit.demo.sqliteclient;

import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;

public final class Repository {

    private Repository() {
    }

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-connection-open","family":"concept:family:sql","function":"openConn","params":["url"],"param_types":["String"],"return_type":"Connection","named_term_tree":{"conceptName":"concept:sql-connection-open","args":[{"sort":"Sql","source":"url"}]}}
    // provekit-concept-payload-cid: blake3-512:b8bcbba05e024511372baca80adb56612a9af00e44e264d7cdbe1bedf3998b489a37bec4b5ab1e73c34d42f6ea7af54756dfcdb1ed61f97a842337de0ad77a82

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-connection-close","family":"concept:family:sql","function":"closeConn","params":["conn"],"param_types":["Connection"],"return_type":"void","named_term_tree":{"conceptName":"concept:sql-connection-close","args":[{"sort":"SqlConn","source":"conn"}]}}
    // provekit-concept-payload-cid: blake3-512:ca83a660b8066c6a1ebd608da824637de0af54cfc12ea114440404c720c651176e8eecc3d8de5918010845cb40fa3565247a6501b60b89a9a97c74d8930dad1b

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","family":"concept:family:sql","function":"exec","params":["conn","sql"],"param_types":["Connection","String"],"return_type":"int","named_term_tree":{"conceptName":"concept:sql-execute","args":[{"sort":"SqlConn","source":"conn"},{"sort":"Sql","source":"sql"}]}}
    // provekit-concept-payload-cid: blake3-512:69628b82c3bce916ce29d6a6a8c1431dabe68ce1041860c705a4e02b54ffdaab43da5ad8a42fec87838ba7e2ddf4ffc63cf66ceba0d54257f0436b0227dd74e2

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","family":"concept:family:sql","function":"queryRows","params":["conn","sql"],"param_types":["Connection","String"],"return_type":"ResultSet","named_term_tree":{"conceptName":"concept:sql-query","args":[{"sort":"SqlConn","source":"conn"},{"sort":"Sql","source":"sql"}]}}
    // provekit-concept-payload-cid: blake3-512:283f4dfb678f91e92113d77e621a51aa215c441e0ccc769b9ead853d70624a9c0b164d3a706157bc34ed6ca1918d0c19bcf8eadad090beeccaee8885368a1864

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-prepare","family":"concept:family:sql","function":"prep","params":["conn","sql"],"param_types":["Connection","String"],"return_type":"PreparedStatement","named_term_tree":{"conceptName":"concept:sql-prepare","args":[{"sort":"SqlConn","source":"conn"},{"sort":"Sql","source":"sql"}]}}
    // provekit-concept-payload-cid: blake3-512:ad7ce09481032f32d9704f11604a43312da366790af229d1d12ada3cf0b1c6922aa8c0cc951aea36f9611f19b6b01d72d01759a8b0b1df4b88f660e173d432de

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-begin","family":"concept:family:sql","function":"txBegin","params":["conn"],"param_types":["Connection"],"return_type":"void","named_term_tree":{"conceptName":"concept:sql-transaction-begin","args":[{"sort":"SqlConn","source":"conn"}]}}
    // provekit-concept-payload-cid: blake3-512:af9dea67332cb31ac3ea14b6d5d67d64c6033874a12aa04cd535db0957528361fde14afd53dc4e42441003cf32e6fc11b8be96aa8685d874d99f8bfc6a59a023

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-commit","family":"concept:family:sql","function":"txCommit","params":["conn"],"param_types":["Connection"],"return_type":"void","named_term_tree":{"conceptName":"concept:sql-transaction-commit","args":[{"sort":"SqlConn","source":"conn"}]}}
    // provekit-concept-payload-cid: blake3-512:a59edb1c942550a6ea6fb172eb8eda22f13de1945ebb74c7ebfeef7a35a14bf5617b8aa609cb3c8c9fbadc36fc6401b6c7fb5b04fc68074a8f5e6caf015491fe

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-rollback","family":"concept:family:sql","function":"txRollback","params":["conn"],"param_types":["Connection"],"return_type":"void","named_term_tree":{"conceptName":"concept:sql-transaction-rollback","args":[{"sort":"SqlConn","source":"conn"}]}}
    // provekit-concept-payload-cid: blake3-512:bc9da8e5f872fe326c825f299ad314248237630a60d0ad8a0a07bbfc967fb29d7dfb3db74055ec590ea7487247241a5c5f8b45f3276969a430fad346714f004d

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-row-get-column","family":"concept:family:sql","function":"rowGet","params":["rs","idx"],"param_types":["ResultSet","int"],"return_type":"Object","named_term_tree":{"conceptName":"concept:sql-row-get-column","args":[{"sort":"SqlRow","source":"rs"},{"sort":"Index","source":"idx"}]}}
    // provekit-concept-payload-cid: blake3-512:f2254848241409db6c532b2d655005397535c7ddcfc854ee34e1c6a85b2e6209d64f8bcc9112bb01187a038433ef4b9056ad9ffce5a30cd4f37df783ebdd31d5

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-changes-count","family":"concept:family:sql","function":"changeCount","params":["conn"],"param_types":["Connection"],"return_type":"int","named_term_tree":{"conceptName":"concept:sql-changes-count","args":[{"sort":"SqlConn","source":"conn"}]}}
    // provekit-concept-payload-cid: blake3-512:04ce7769f1f2260537e2fdbc66a97e00fc11492f6078bbcd232a23f4adfdec91e3df5c51569cb437dfe0d4b1aea1131994b552fef0c310bb7c8041a278a31f5a

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-busy-timeout","family":"concept:family:sql","function":"busyTimeout","params":["stmt","secs"],"param_types":["Statement","int"],"return_type":"void","named_term_tree":{"conceptName":"concept:sql-busy-timeout","args":[{"sort":"SqlStmt","source":"stmt"},{"sort":"Index","source":"secs"}]}}
    // provekit-concept-payload-cid: blake3-512:8a2ee564c5952914c862dd40ecbe9a8a8fd8bc5b67aaa7a85c4072437401f620debaf2847b2c11783b037f04607ae813476c1df0d39ab63862949ff5c9ed1c01
}
