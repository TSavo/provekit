# Concept Shape Catalog

This menagerie exhibit is the node table for universal algebraic shape addresses beyond `foo`.
Each recurring cross-language idiom has a universal address: the CID of its algebraic-shape contract.
A lifted instance from any language or ISA lands on that CID after the renaming and representation morphism is applied, because the canonicalizer deduplicates the normalized contract bytes.

The CID space is treated as a self-organizing map, a Kohonen map without loss. The catalog grows as more idioms are named, and it plateaus the way the contract catalog plateaus: there are only finitely many cluster centers, the recurring computational idioms.

`menagerie/foo-algebraic-shape/` is one node in this same space. This exhibit names the rest of the starter node table, and both exhibits grow toward the same plateau.

## Node Table

| Concept | Shape CID | Realizations | Discharged Morphisms |
| --- | --- | --- | --- |

| `allocate-or-bail` | `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c` | C, Rust | morphism_c_allocate_or_bail_to_shape, morphism_rust_allocate_or_bail_to_shape |
| `check-bounds-then-access` | `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f` | C, Rust | morphism_c_check_bounds_then_access_to_shape, morphism_rust_check_bounds_then_access_to_shape |
| `acquire-use-release` | `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4` | C, Rust | morphism_c_acquire_use_release_to_shape, morphism_rust_acquire_use_release_to_shape |
| `validate-then-commit` | `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae` | C, Rust | morphism_c_validate_then_commit_to_shape, morphism_rust_validate_then_commit_to_shape |
| `branch-on-error-else-passthrough` | `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b` | C, Rust | morphism_c_branch_on_error_else_passthrough_to_shape, morphism_rust_branch_on_error_else_passthrough_to_shape |
| `refcount-inc-use-dec` | `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e` | C, Rust | morphism_c_refcount_inc_use_dec_to_shape, morphism_rust_refcount_inc_use_dec_to_shape |

## Primitive Operation Hubs

These are primitive operation concept nodes, not idiom shapes. They are the minimal hub used by `provekit transport` for the C-to-Rust `foo` path.

## Exam Manifests

`exams/` is reserved for `ExamManifestMemento` instances. A manifest is the stable, content-addressed question set for a concept hub version; it does not carry coverage state. The v1 shape is specified at `protocol/specs/2026-05-16-exam-manifest-memento.md`, with the concept shape at `specs/exam-manifest_shape.spec.json`.

The v1 manifest is minted by `scripts/mint_exam_manifest.py`, which deterministically projects primitive shape specs, abstraction catalog entries, sort instances, known HTTP and SQL library tags, and effect signatures into `exams/v1.<cid>.json`; the minted CID is registered in `catalog/index.json` with `kind: "exam"`. Coverage state remains separate in the deferred `ExamCoverageMemento` family.

## Concept Details

### allocate-or-bail

- Shape: `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c`
- C source: `blake3-512:1e148e4f3e5fcb5014cb127a58160efe0002c29bdb6963ddbc22ae56347137d6d544d8bffd9c9dbccb9b8a84a7094645d8bb8dbe5880bb8afbd5e71e7f520df8`
- C morphism: `blake3-512:50452b65547e8923957e4f2a9aaf0d538e715f583e60b4e36f31043afc1e977441e352e939fc5eef01c020a3998af85dab8ec188cfc1d2ae4eaf17536e8400ed`
- C receipt: `blake3-512:a980d4bed06f87e298529590dfd56d4dda9e888b332328826dd23805e586e0f34c9b81ca6a0422ef8641cfd1a80da261c98cd30e107a4dc07579902afb3b19bc`
- Rust source: `blake3-512:63a50f4d06b00a21d403b8fb9d0e51b49e8d5044d751bd23d6329aa2a993ff78a06f9ab448711b7d5d367391f8e2ebae0e31e18a8df66f7dad883067ed150a35`
- Rust morphism: `blake3-512:c72a08c3f11b839934e8aed4456e1b105e33ef6ff91b77fa293a5456eff8a4af920300ba5eac4c745ce2c3b3f57833057c69246b6a664e0d0f2423262b487e45`
- Rust receipt: `blake3-512:2835c72d9369f79beff8a34722b1b81d11e91d0ace1df28e576bae5d9a9cd71594e8a8f7f575b6ba95f776e54e988aa662a9ec4d4e686f13e5bb35d3b2e5b35b`

### check-bounds-then-access

- Shape: `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f`
- C source: `blake3-512:2b6a0e6a7b696c43680723cd54394a6d55e3dcfa4bfe7f774b29f7afe997324dabd49b6529086cfdcc09b2f616c43f6832487c79a39a48ab6d0b6f53412e6752`
- C morphism: `blake3-512:90ca9d2dde2e9acb8309f5c20b5a5008b4b2453a3048784a596bbeb9ec258360ff6222c2be098c85c0b4ce44efc03b9612aad0afbcafc3227dc3d0e6ecdca544`
- C receipt: `blake3-512:a9f6ea087ffe3d3e5e130148ca89223f254b1c1bb1d6a54165a830f760c224bbd8ffc382ce1e0bd8c7dfb412f10b2dfacd3a1bf5273fd1c7d723671bc3bf8c7a`
- Rust source: `blake3-512:4dcce7100f6a5756ff6e7b920b5ebb2648cf2ce839b26657e512554a752334163d524591dd58e4cff8f63de7db553dea1cdbffa6bbdfefb588dfb465672614fa`
- Rust morphism: `blake3-512:21db13bca50174cfde6f83a76d72c32d25f480072fbe83aeaf0ba8bffae8f80ff725f608d24cd3c2235c792e4fecc0a7253d46f9d94faf85cceb0e193fc33fe8`
- Rust receipt: `blake3-512:203495df99d6b92b4080f6ead334b1d0afbf086751e53827720860c3a081f274e9500279a38cd554f930f4884b217cd45c653a01c881c11f1f47d787e33f960d`

### acquire-use-release

- Shape: `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4`
- C source: `blake3-512:7a71116f6f21bade4739d9fd066f5352b1ba739133bf01a014bfe1c38efdc332326ad06f4a495d81bf194d604fc95715d48d7ae9e8e649c1b9512156d72a2a6c`
- C morphism: `blake3-512:561b53854df5ebf79f21701c4ff8f4d4acda49b856e7f25e4fdbb4373de03a4fd8aeec9a063fb66caca0c6c462eae111d4d8706d7302778dad81116c7cf3c14d`
- C receipt: `blake3-512:6cd06177e8ccbfaf95b923b2e1b9ddbcb1538cb302f2990986811b45ca555af958d37f6ba586a70ff9ee4cc25034e13fad4be5d32b3dfd44cf443533da780fc7`
- Rust source: `blake3-512:52b20b82b6caabf4ada8443d07160013268b8beab4888e871772d0608ff2394468360fba9dc7fa9573cb0796e2f7231ce0648c4cbeb2b44c8732c4ff84f5efb7`
- Rust morphism: `blake3-512:a7e62ad34a151ed439c5902857c54bb23906cd578a47b327e9769541c8447eacbba6b7ff42d47bc868752392bc7896f1f4b0cebb15211895179307df239baf85`
- Rust receipt: `blake3-512:25b0530104f7e4b7da3f54d3bd261d5021a642d8123208bd4d22b23a547c6c297515809fcda301d39c44123da32e8d7b6b61bc6ab24213fa2e61eec74130b2f1`

### validate-then-commit

- Shape: `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae`
- C source: `blake3-512:6449d1d013c7c8b568abfb5c6e8cfa98e868869accc21f6bfe39037e5ccbaedcc6eb04be1b0e9a7ac2e10d97f92ba90c0f9758321f700fa48da0f56cb11f7722`
- C morphism: `blake3-512:9ce2a7d273c005c6e5d0936deae5e4cd6783a72cbf3d007c8571a0dd74091e61c3a3a3feefc1093f54f051f5a75f9668cd0c3bcd59f989192d31c0e5c1bfe307`
- C receipt: `blake3-512:c793b4cd247b9e00051d1015a876c560a23ecad7233aeea4dae544d12522011e7a64d930a32c0698269cde872d752e66faf12016eb0e95228e173f83fbbad9b8`
- Rust source: `blake3-512:6fe082243005cf1b42dee0e38c8ec07d761fc60b8e7887abfe8e4002f2d4dfca655fd6f9d0fbaab172a75fea5d21beb7a1f03918611ec81481a70a6dc5e329cc`
- Rust morphism: `blake3-512:e8577df2a02ff15be5c9a4c200f828354fda8da2bb3a5148a3b102b315d52ba78aa5970b2d3f5d7777216fb026d0e866901347afe5bf22976dd01a742a26b288`
- Rust receipt: `blake3-512:82265e10ef9d7aecccca24f4470183bd27847b85f3d28d81e5ea0a2084688b03af363c5e00c120fe056a215e7a3fbaf981186364b3124c3c5cb17d928cbcc4d8`

### branch-on-error-else-passthrough

- Shape: `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b`
- C source: `blake3-512:5f474e8af37af7a6730fbc704fe4350059c2334c948d7e34489ed4f41531af1cdd10fc87bd29bc6b697d89f7d82e999ab690f80d271d15fec8ccf03016ebdeb2`
- C morphism: `blake3-512:08e136f0b16268168cc70ecb6095d1d5643c190120ed9a08d8ef70161fb9b5eac02dc03f139a21c8581a2f90bc9092ac0987ae9dcd288c1ddd865a41d4ca1975`
- C receipt: `blake3-512:2a75cb1b90b46a75430a5a1233b456ee03209ec7ccb77d24cf5c665525ff57fd14ea98150c359dccc2b0dffb81490512ca39054c6a48979f48f09c5c56daeac8`
- Rust source: `blake3-512:17f22114007c8d37afbbdf65605824b6ee04fde2880f63a52c3decab7d63795408cfa2a9f656ce0802e96c3f6a9ae88ed4ce55d8f04d94ff15eb361c5dcb910b`
- Rust morphism: `blake3-512:28b3e7c54d412f9e399198a58a2a28cb2be0dcf6b7db1778cc752aee933631ecc677d53f31576b78f4f997175e3eecb9e19f1626c9c416e45a886d3c06ca10cd`
- Rust receipt: `blake3-512:d316a0476a8cf5ea7e9d332753d2bbda4f550477fe9ff6d87a09a3edfd89c58b8ba207c28a0a91082bca7d4194694277283c09ccef959d172a3793994808100e`

### refcount-inc-use-dec

- Shape: `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e`
- C source: `blake3-512:168fda3643fac5e0dd75c687613d3a00066d7e66f8f2d953dee7eba357ec59a069b4fbdf0e25bf3abc3fe44dd311f2ba64c76d3f33c2542cc322413837e5ba4a`
- C morphism: `blake3-512:845865f123e8983e6d4a989288330e1b96cacbae723b36f4422fa61b50e8f7c988c647926aeaec977d601274c3ee2205457f841396685358cad2a4f2efa7045e`
- C receipt: `blake3-512:602a9344cf960bd1cf5de57e0f57de4e6fb04999504336d359a3f56973a0e6f5952f471bcf712aba47ea0e16eaf1ed8df92afb6e15e867f50719a75a8b8102bb`
- Rust source: `blake3-512:aa480223c50f8061c23a35894ca2a407e420c036b488d043b298708cefbf47ba1c019d66fda7956f2bf4902861b4d4572afd433f7efd2c6a6ffd498e2529f8a1`
- Rust morphism: `blake3-512:26e325bf56a11e3b4ef0887ced6ae15b58d4b1ce5fbb0855333bcfb33cf6a3854f8df46e486724e20de59f66877e4ae1c0cb37442c87ed38dde3d15a874ad1da`
- Rust receipt: `blake3-512:cda001205044132fe247b20f0836acd8bfc918cb0923eac6f49e1921dab26fc9eaee98d7df4754f33f9686aeeb34f8a568acdd209c85641c8d9600dd8e5ca61d`

## Foo Specialization

`foo` is the instance of `branch-on-error-else-passthrough` where `err_cond(x)` is `x == 0` and `err_val` is `-22`.
- Foo shape CID: `blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1`
- Branch shape CID: `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b`
- Specialization morphism CID: `blake3-512:a33706b4f08d6a9353b1b3223bdf9ea586593263efe4d6fb735b31fd3e68ee85859a8d4d0949ad0c792369ced097367c9896fb6f32ec12f2a7d684e8ec611ec9`
- Specialization receipt CID: `blake3-512:f79ec89e0f847a9d7e1576c1abd311b7d966b3a4866e2c6807703775b10cbb3d9bcd938a1370275760d65a04b6cda29f4486d419b623b524e6afb9c02dd622dd`

## Conjoinable Shapes

Shapes are conjoinable. `allocate-or-bail` followed by `check-bounds-then-access` on the allocated buffer composes through CCP into `validated-allocated-access`, a composite shape with its own universal address.
- `validated-allocated-access` shape CID: `blake3-512:bbb9e01a79af15d27da8772bf27d822e161defe17672c37b793c2200ebe85e2452e8ab5595e8e825589d9f1cf27fed560c0dde8d43496598145ad3d9478e9152`
- Composition receipt CID: `blake3-512:ebcada85f7394894446e58760b8b3b3370974664dd423e72142fea9cbf36b33313b9503ececfd3fb862eb075076d5031e33f18a79c7f1759cb40f3ce054c6a95`
- libprovekit compose probe CID: `blake3-512:46459256c7797f637370eee21ff1efdf4799b4ba72a0c3c63756c9f951ecfb54674d4ff5d118cef039d1c74d377a04e41c5b696a4a54bd9c1cc362455c83a351`

## Discharges

| Morphism | After substitution CID | Shape CID |
| --- | --- | --- |
| `morphism_c_allocate_or_bail_to_shape` | `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c` | `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c` |
| `morphism_rust_allocate_or_bail_to_shape` | `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c` | `blake3-512:74c5dbb37b69436b3dc186628326b52f4a000421e1385cba8bc52f477609d5f4ad273e852b2a45ad7a1ade8ef4f54db0b690e6a233847499736927967d17330c` |
| `morphism_c_check_bounds_then_access_to_shape` | `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f` | `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f` |
| `morphism_rust_check_bounds_then_access_to_shape` | `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f` | `blake3-512:cb37f973e9b92c56d77bb7fc43ae097ddcfea5ada234f41e3145c80e10074a0f3f38b848a8cc8f907bc17dab7a2385c3ef5673eef3eee2e11efae716016b8b6f` |
| `morphism_c_acquire_use_release_to_shape` | `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4` | `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4` |
| `morphism_rust_acquire_use_release_to_shape` | `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4` | `blake3-512:caac0f85ee27cfed8ebc894ac69b0ec6f31da3f274f7820fbb04be6751c1618264a22380fb3dfc55d12b1a6894201f95f412ba36261cf2eef8132ce8553069c4` |
| `morphism_c_validate_then_commit_to_shape` | `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae` | `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae` |
| `morphism_rust_validate_then_commit_to_shape` | `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae` | `blake3-512:79bc915f87dacb3902b5c0a69fc9920b662bb3ed15d35066040d92b98daa46c2c31af84c3b6c0c5e34d778f57091127bf8afe1a5eb9d9152459027bcf946c3ae` |
| `morphism_c_branch_on_error_else_passthrough_to_shape` | `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b` | `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b` |
| `morphism_rust_branch_on_error_else_passthrough_to_shape` | `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b` | `blake3-512:320d5c06af9642dee5896aee14dfc1e87f9f2796ca5c16a6a743b5d9f49ed06fcc3a7a71c83c8c0f0724bf6f327368135187382bddd497aee91d11a48d73504b` |
| `morphism_c_refcount_inc_use_dec_to_shape` | `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e` | `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e` |
| `morphism_rust_refcount_inc_use_dec_to_shape` | `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e` | `blake3-512:2b334aca2238b2a55c6e34f705b84e9f5abbbc34aa47b7310a92b73e05e70fd3072bc7315e9a55c2f2ed15826af0c68be18b95c2d07b9b9971f9d5041ca4916e` |

All after-substitution CIDs above equal their target shape CIDs. These are canonicalizer discharges, not solver proofs.

## Reproduce

Run:

```sh
menagerie/concept-shapes/mint.sh
```

The script builds the Rust CLI and canonicalizer helper, writes concrete source contracts, mints shapes and morphisms into `catalog/`, writes receipts, updates `cids.tsv`, and scans this exhibit for forbidden dash characters and the forbidden sign-off name.

## References

- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`
- `protocol/specs/2026-05-10-realizer-protocol-v2.md` (ORP v0.2)

T Savo

## Common Imperative Program Transport Hub

The `concept:*` operation nodes below are the common-imperative core used by program transport.
They are operation-contract shape mementos, not language-prefixed operations. Per-language morphisms are minted from real lifter-emitted ops by `scripts/mint_language_morphisms.py`; ops that do not discharge are recorded in `transport-gaps.md`.

| Concept op | Shape CID | Minted morphisms |
| --- | --- | --- |
| `concept:add` | `blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468` | morphism_csharp_add_to_add, morphism_go_add_to_add, morphism_zig_add_to_add, morphism_java_add_to_add, morphism_rust_add_to_add |
| `concept:sub` | `blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af` | morphism_c11_sub_to_sub, morphism_csharp_sub_to_sub, morphism_go_sub_to_sub, morphism_zig_sub_to_sub, morphism_java_sub_to_sub, morphism_rust_sub_to_sub |
| `concept:mul` | `blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03` | morphism_c11_mul_to_mul, morphism_csharp_mul_to_mul, morphism_go_mul_to_mul, morphism_zig_mul_to_mul, morphism_java_mul_to_mul, morphism_rust_mul_to_mul |
| `concept:div` | `blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839` | morphism_c11_div_to_div, morphism_go_div_to_div, morphism_zig_div_to_div, morphism_java_div_to_div, morphism_rust_div_to_div |
| `concept:mod` | `blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d` | morphism_c11_mod_to_mod, morphism_go_mod_to_mod, morphism_zig_mod_to_mod, morphism_java_mod_to_mod, morphism_rust_rem_to_mod |
| `concept:neg` | `blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409` | morphism_c11_neg_to_neg, morphism_go_neg_to_neg, morphism_zig_neg_to_neg, morphism_java_neg_to_neg, morphism_rust_neg_to_neg |
| `concept:bitand` | `blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b` | morphism_c11_bit_and_to_bitand, morphism_go_bitand_to_bitand, morphism_zig_bitand_to_bitand, morphism_rust_bit_and_to_bitand |
| `concept:bitor` | `blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3` | morphism_c11_bit_or_to_bitor, morphism_go_bitor_to_bitor, morphism_zig_bitor_to_bitor, morphism_rust_bit_or_to_bitor |
| `concept:bitxor` | `blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353` | morphism_c11_bit_xor_to_bitxor, morphism_go_bitxor_to_bitxor, morphism_zig_bitxor_to_bitxor, morphism_rust_bit_xor_to_bitxor |
| `concept:bitnot` | `blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f` | morphism_c11_bit_not_to_bitnot, morphism_go_bitnot_to_bitnot, morphism_zig_bitnot_to_bitnot, morphism_java_bitnot_to_bitnot, morphism_rust_bit_not_to_bitnot |
| `concept:shl` | `blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a` | morphism_c11_shl_to_shl, morphism_go_shl_to_shl, morphism_zig_shl_to_shl, morphism_java_shl_to_shl, morphism_rust_shl_to_shl |
| `concept:shr` | `blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b` | morphism_c11_shr_to_shr, morphism_go_shr_to_shr, morphism_zig_shr_to_shr, morphism_java_shr_to_shr, morphism_rust_shr_to_shr |
| `concept:ushr` | `blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad` | morphism_typescript_ushr_to_ushr, morphism_java_ushr_to_ushr |
| `concept:eq` | `blake3-512:6416cb7457c857c60cd901a01edddb95fed1cd8f890ab6a37c874edcdb7125e5308cfdba63247e84abde588e4f555ebdd4020f7362a3f7c8746bae338f57d522` | morphism_c11_eq_to_eq, morphism_csharp_eq_to_eq, morphism_go_eq_to_eq, morphism_zig_eq_to_eq, morphism_java_eq_to_eq, morphism_rust_eq_to_eq |
| `concept:ne` | `blake3-512:c2b629f979bd457b4945c6adc1552edfd48f2a65acf233eaaa02a7f90afcaf8542502c96826fa46d422b56ebc3436018b34cde478122bd4a9135cddf70e07b19` | morphism_c11_ne_to_ne, morphism_csharp_ne_to_ne, morphism_go_ne_to_ne, morphism_zig_ne_to_ne, morphism_java_ne_to_ne, morphism_rust_ne_to_ne |
| `concept:lt` | `blake3-512:3a4a311ed57e6cf96508cfb5b9e4456716bab5a6eaa4ef43ee5163e5592b28b6da1844d64da78cf36bdcf90eaaf07de82ed51851d9668cb9dafe0cfe2a52a20b` | morphism_c11_lt_to_lt, morphism_go_lt_to_lt, morphism_zig_lt_to_lt, morphism_java_lt_to_lt, morphism_rust_lt_to_lt |
| `concept:le` | `blake3-512:456f76c76a128069bd4970d4009e5f7e0d8938e009c2b096cd9612a6b50e2b4eac33d4c09d36e31bd7925c2ba8c3bf152b80ed559100740ceafeb8dad8be0985` | morphism_c11_le_to_le, morphism_go_le_to_le, morphism_zig_le_to_le, morphism_java_le_to_le, morphism_rust_le_to_le |
| `concept:gt` | `blake3-512:e33869585ba33724173a12d34efa55544c133a4cc5a29d788e343fb7d9d4e06f7a8dd965a2492c989586989d84e3337d493d0704c34a9c486c4d72257a134e44` | morphism_c11_gt_to_gt, morphism_go_gt_to_gt, morphism_zig_gt_to_gt, morphism_java_gt_to_gt, morphism_rust_gt_to_gt |
| `concept:ge` | `blake3-512:ee6a800eaf4d13e80f06f936bbaa86a0a80276ce8d1ab8bc7440d194e0e2034ba8a74fb5c74e624a25f2c0bb76e288191ce4e282f1fbbefed91c390ff8a1dfeb` | morphism_c11_ge_to_ge, morphism_go_ge_to_ge, morphism_zig_ge_to_ge, morphism_java_ge_to_ge, morphism_rust_ge_to_ge |
| `concept:not` | `blake3-512:74aa536fdfabbbbf60e1b5753381e7cebe4ae808dc0e3be6ed0d7aa1c231032e1a5ff3fd82939b1fb3096d8f92b832ec89c71841ea7f4cdf3a8b4c1481e789b6` | morphism_c11_not_to_not, morphism_zig_not_to_not, morphism_java_not_to_not, morphism_rust_not_to_not |
| `concept:assign` | `blake3-512:c92de23581656b0f75bdeff5efe428c33b0088c43f28a35fa50ad77b73f5e13f60078d66c22cfed80d7764224d5647522e92d18de9e8808a19ae31fc4b1b389a` | morphism_c11_assign_to_assign, morphism_typescript_assign_to_assign, morphism_zig_assign_to_assign, morphism_rust_assign_to_assign |
| `concept:decl` | `blake3-512:8fa95384a32b13dc6589299f30fcf78e2a1b8dc3be188a04ce7edfaf988b3c7aae3420447e2bb1ebee1a635e1d51df2f807bf159d1ca35e77741aee131faba84` | morphism_c11_decl_to_decl, morphism_typescript_decl_to_decl, morphism_zig_decl_to_decl, morphism_java_decl_to_decl, morphism_rust_let_to_decl |
| `concept:seq` | `blake3-512:2f4f01ae873fcd3d57990ecd94e24caaa3ce1cbd5baf4347b21fe8625750bd4b30e2f92b8415e4595ec637d3aa7f511d42c006d587ee1cf5fe8b264df39f1504` | morphism_c11_seq_to_seq, morphism_csharp_seq_to_seq, morphism_python_seq_to_seq, morphism_zig_seq_to_seq, morphism_ruby_seq_to_seq, morphism_php_seq_to_seq, morphism_java_seq_to_seq, morphism_rust_seq_to_seq |
| `concept:skip` | `blake3-512:9a905548a44fce23882b17d857d275d7822bd235ab71dbf786cd991563cc1de9e610594f50ad3c89a3b7eeb43234a31b36caa8031914c85227158030669c63cb` | morphism_c11_skip_to_skip, morphism_csharp_skip_to_skip, morphism_zig_skip_to_skip, morphism_java_skip_to_skip, morphism_rust_skip_to_skip |
| `concept:conditional` | `blake3-512:40d7790bd82a90175e5ffa7f0a3b4587c274d60a35065693eaa503f68bf8d1039839eb6687831dd7a66c9b45fcb57ec9eac68cf01f6b600afe6dd5a832c76492` | morphism_c11_if_to_conditional, morphism_csharp_if_to_conditional, morphism_python_if_to_conditional, morphism_zig_if_to_conditional, morphism_ruby_if_to_conditional, morphism_rust_if_to_conditional |
| `concept:ite` | `blake3-512:3c87b90507933effd400bd2ddc2e15430f55dcbd466f351b7bb96140015bd228faf54efe31032114d4d5dd255188c8d1dcf202b2267fa09db0560d40c1ec5bab` | morphism_c11_conditional_to_ite, morphism_java_ite_to_ite, morphism_rust_ite_to_ite |
| `concept:while` | `blake3-512:57b69cf351659f32356e3bfd6904171ae539776e0c0f60e25b330106ffc4b63a0854cb3d0716bb9198d6f53a2480d3b681dbf97c9be9a38f9c5f88ab3caa16c1` | morphism_c11_while_to_while, morphism_csharp_while_to_while, morphism_python_while_to_while, morphism_zig_while_to_while, morphism_ruby_while_to_while, morphism_java_while_to_while, morphism_rust_while_to_while |
| `concept:do` | `blake3-512:8531b1d03564cb98d0c9dda92f973048b2451e9b4ad830e7f2fd8cc1f031e14cc96cba2db884c9ae6001a126c0c48463b5b531ab829399168d56df51050b0041` | morphism_c11_do_to_do, morphism_java_do_to_do, morphism_rust_loop_to_do |
| `concept:for` | `blake3-512:5efdbe33b64f902d53f221eb98e33511b98766333bc10042982a0f943faf1cb300bc5a0b6018af296c37f32fd0eb11a170bae1d06e42a604f85475c0f98eb16f` | morphism_c11_for_to_for, morphism_csharp_for_to_for, morphism_java_for_to_for, morphism_rust_for_to_for |
| `concept:break` | `blake3-512:2a222e362b6dd8899e861a38e66d2a584904e13ec6df85d3077b7e73e08e48a075e86a54c70acb30c184abb5b6a8a6d8449dc9df268dd2ae161d8927c71f21d1` | morphism_c11_break_to_break, morphism_csharp_break_to_break, morphism_typescript_break_to_break, morphism_zig_break_to_break, morphism_php_break_to_break, morphism_java_break_to_break, morphism_rust_break_to_break |
| `concept:continue` | `blake3-512:50144af34411d91c720017f210c587b514e029d79490f86d60e5679bd945a922cd2b6bc160bd8d1c6e0e89ff8063c043f00960401e3e9e617b21b48120cbefbc` | morphism_c11_continue_to_continue, morphism_csharp_continue_to_continue, morphism_typescript_continue_to_continue, morphism_zig_continue_to_continue, morphism_php_continue_to_continue, morphism_java_continue_to_continue, morphism_rust_continue_to_continue |
| `concept:return` | `blake3-512:29e5e9e537f55d701ce6d342fde2c39a4b7158655c77ee08788fac213cbd866cb308f283e0852fccecdc21f0bfdf533fa0592008d80449a5db1a2c265d362f9f` | morphism_c11_return_to_return, morphism_typescript_return_to_return, morphism_zig_return_to_return, morphism_java_return_to_return, morphism_rust_return_to_return |
| `concept:call` | `blake3-512:fa2fd7c6f33492f270282faf69a89e21bb9988d8d0d9678d253c19aa00a977bf1158396b870f2160b718835c6189b51a97b848af8946d43e4244728f0b7e870c` | morphism_c11_call_to_call, morphism_rust_call_to_call |
| `concept:index` | `blake3-512:18e36040cd6ef5f32811245338ab550147d76a5a1a3e525e6bdba05a492460b3d68d00c49abd9b9e4369698228780d0907b2ca367b783ef045859f8d7ed4cc12` | morphism_c11_array_subscript_to_index, morphism_rust_index_to_index |
| `concept:member` | `blake3-512:8c9f60571f0b644a09ef99ae2779927221cd61a7ba20000c0c933ac01b3fa4b1c41658df47404b49a0ef50006fd04139e7dd90f6352d3fc1aeb9b1c6c3677271` | morphism_c11_member_to_member, morphism_rust_member_to_member |
| `concept:deref` | `blake3-512:93ff252a879bc061949fecdb9710a0a927b47f5104f5e628c7e0bd2477e3ea3515ebb2bc2794d9cc7c11c6ea16db511ff20a18c699bb94f7854e79b5e195f717` | morphism_c11_deref_to_deref, morphism_rust_deref_to_deref |
| `concept:addr` | `blake3-512:5d632655150ebe3b7b5b828bb86a7d051c57d5110794375a2e1f707eb69a3948586d9a0ffe575e6eefe0c99f462bea62904e7b1491a556c975626233b2c140f7` | morphism_c11_addr_of_to_addr, morphism_zig_addr_to_addr, morphism_rust_borrow_to_addr |
| `concept:new` | `blake3-512:26eb0a9484d68fff3fafe1ee82f09e3c3f49e1e2d1e8d01c733362b39473590e61f5903080ffdf69f2532e57047d0fbd4439a11ff778936e27a61f0c4c8c35b8` | morphism_csharp_new_to_new |
| `concept:cast` | `blake3-512:f410b454baa33f207b03cea78723ce7f457253c17959ca1d54c611e63e3260d2a5142ac927945118ad8b880cbcfbee16995db334c32f1caabe3a3677c59277f5` | morphism_c11_cast_to_cast, morphism_rust_cast_to_cast |
| `concept:throw` | `blake3-512:bfca9b128ea5128d15236ebbe44150ff60355b9bbcd664ae4abbc34f2e4e658f7441089449956bfdb333d1f2eb1bff828c74a5d2f3df7fec723abe883bb81a12` | morphism_python_throw_to_throw, morphism_typescript_throw_to_throw, morphism_php_throw_to_throw, morphism_java_throw_to_throw |
| `concept:postinc` | `blake3-512:be615743882f980a2fde0ca6ec3250305c28e2fac1fe4d17accd1790d62af7992ff80282f6507335b959ccceaa32a047f1845b8a9e96a54d20b3766d46589aee` | morphism_c11_post_inc_to_postinc |
| `concept:postdec` | `blake3-512:cac33b2bef01e38d327440e7bfecebf3e7540d463a02e68dd047e47d0c9cca45f94181ce773fb389671a960cc957760b540b2927afd6d2c624cf9ddaca225f1a` | morphism_c11_post_dec_to_postdec |
| `concept:preinc` | `blake3-512:8c8383c221eaca3b95d30437d768065d5117091415afb04e92f541af6fb26d37af79d423e25a59ffaf3f6e2d654d0bd64cfe8e071ee5483ed6bca2614442001f` | morphism_c11_pre_inc_to_preinc |
| `concept:predec` | `blake3-512:fa83fc84643e03f1e60aa66848412e0cdc25ad6ede0cf216643fb8d4dbe52c4d8df28283f754040cc0f53a62ec22e73a2db623e6507055ab1076df8394024995` | morphism_c11_pre_dec_to_predec |
| `concept:source-unit` | `blake3-512:377bec17d4c9ea2e44216e244685c282b3ac83c19191699eab94a47ff0b123bf4899d6b5691ce88fa7bc70d4dd9f8d2566631bd02895f891ec67ca6d32a87285` | morphism_c11_source_unit_to_source_unit, morphism_csharp_source_unit_to_source_unit, morphism_python_source_unit_to_source_unit, morphism_typescript_source_unit_to_source_unit, morphism_zig_source_unit_to_source_unit, morphism_ruby_source_unit_to_source_unit, morphism_php_source_unit_to_source_unit, morphism_java_source_unit_to_source_unit |

T Savo

## HTTP Concept Shapes

Bridge B mints the HTTP request and response concept shapes used by later HTTP sugar and trinity payload work.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:http-request` | `blake3-512:784dab96537ebae452cba5fdbcf88e07395d5e0634099055008d819f21d0fb51930fc29877afda069cdf0c1ec893fba5de47b025717fd024919c687381baee43` | method, URL, headers, optional stream or bytes body |
| `concept:http-response` | `blake3-512:38a31226e5e2f593fa12b1e7a2b18d9f7755301ce537115b34ac486aedcc479ca599327dbea7de0e0cee0d035b831ad4933436c2b7c8c84d4f4694dc42d161f5` | status, headers, stream or bytes body |
| `concept:url` | `blake3-512:ec39f4820bdac0fc1bfb60e30d7348e2273a89b0f13f7fd3be49b03d206026e5a1f9414aca64ac639aee6ec789dbbd2045309146d118e66c8bccd5b8be654463` | parsed URL component carrier |
| `concept:header-map` | `blake3-512:53cb0113c4211d9c326868d5901d0c7e699f4ae23078bea4de54086c1cd59e92ca2accbd5986db0643b7689ea57011e3ed33514614f3626d774fd5cbf011cab7` | duplicate-preserving header multimap |
| `concept:byte-stream` | `blake3-512:d5c8fe062ffe4004ccef5c49a90eaebe4f2a3cf46c315c3d966d0214c9ee5311a94355a103094669ea4634bac0fb0e82e703eb35fcbf174810defa86ee02c128` | ordered byte chunks with optional length hint |

Examples: `examples.md` in this directory shows high-level bindings for libcurl, `java.net.http`, and `urllib.request`.

## Contract Observation Concept Shape

`concept:contract-observation(callsite_cid, contract_cid, mode)` is the hub op for witness, monitor, emitter, and gate observation wrappers.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:contract-observation` | `blake3-512:a1bc7493bb1173952c45ea6c25110195eb721733fba685cecada22543b7b543ba4366f06ec0294acb7402c2c27baa24329f1b85fe59a27af87da009ca3f1f063` | mode is a formal slot; observer effects live on the wrapper memento, not the object FCM |

## Fully Qualified Path Concept Shape

`concept:fully-qualified-path(path)` is the hub op for Rust paths whose module, trait, crate-root, and associated item segments must survive lifting.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:fully-qualified-path` | `blake3-512:d715caff02a1db7e15cc4e290496e9696c5502c89eeced222878c391470a10bb410b5ea59bd7139a1cef720dc49a2a949804baddc93911c5222eee73ab6b9a42` | carries the exact path string as the substrate term payload |

## Log Emit Concept Shape

`concept:log-emit(level, message, structured_fields)` is the hub op for logger-agnostic monitor and emitter body-template composition.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:log-emit` | `blake3-512:e52923ef4ccdebb96c7a2127ae7a0053227737664615f21716da723975042a6c5f2689bd467fb1cf2e2e8280473fe361b443b6d69306723664f806e5dc848333` | effect is IO; per-language logger sugars carry honest loss dimensions |

## Comment Concept Shape

`concept:comment(surface)` carries source comments through lift, bind, and lower as trivia with no runtime effect.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:comment` | `blake3-512:d9c806063bb97d59ca655b6c50b6ad2ff4cbadd02d6238a51a33a63ec6626af6d92e338ca10f9598fa322cd960d007349752f477fdf3a9384491282d8d12fef2` | surface is the raw comment delimiter text; formatters own whitespace |

## Op Definition Concept Shape

`concept:op-definition(name, arg_sorts, return_sort, effects, wp_rule)` describes a hub op definition; CCL's meta-layer primitive.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:op-definition` | `blake3-512:bbbafea8e3c6d3b24e183a1a2254f1645d8520c6a48571f2631e4fe6516db8d03e8109f31152d789401ea8ae57f30eacb1797db9c53495c25e5d14344c48e852` | describes a hub op definition; CCL's meta-layer primitive |

## Op Application Concept Shape

`concept:op-application(op_definition_cid, args)` describes an application of a minted CCL op definition.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:op-application` | `blake3-512:b98ae1ac8b8f06800db51e648c3a3567200b73e256fcb4d3f664262c95e928979b93f946362c88c14c91d57e0daaabef4dc97ba8617b39828d802db153b36981` | applies a minted op definition to sorted term arguments |

## Core Sort Hierarchy

`concept:sort(name, kind, generic_args)` describes the core sort hierarchy primitive and the initial sort instances.

| Concept | Kind | CID |
| --- | --- | --- |
| `concept:sort` | `shape` | `blake3-512:8d9e3d54325e7a123528a38f7fc268c64a2dfe9a43fca08a234dd530015c7e53f89510093404db9ab63cdcfee59fe1de712f1b7ac6a736475ae2f090c1d2eab0` |
| `Int` | `sort-instance` | `blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58` |
| `Bool` | `sort-instance` | `blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074` |
| `String` | `sort-instance` | `blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10` |
| `Bytes` | `sort-instance` | `blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b` |
| `Float` | `sort-instance` | `blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57` |
| `Null` | `sort-instance` | `blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5` |
| `Cid` | `sort-instance` | `blake3-512:4a2bba3a8207f364e0ffea40fb4ea4c7ea2ce6edac9492e8b0364001821978a37d9e3f782587b19f086c7358c35bc237815f7a617b61c9194db93aaf938c9c48` |
| `OpCid` | `sort-instance` | `blake3-512:3238b8edabae57231223e24f21644b3fa7720cdef57b85f548cd4946ff7b279a6a40eb0d7f064027888aad9a78e5ba0b04a432c83a16b56c7a34c6faf5cd0ba9` |
| `SortCid` | `sort-instance` | `blake3-512:f5c63d82c8fabb1a2e4dd3a5b01cf5f22bddd8252b92602f18bed37062d76334a77f24941a95698c52836b6f11704ce1e7568f95493a5ebaf2b0836e291e97b0` |
| `EffectName` | `sort-instance` | `blake3-512:2e8e8e72cb8563447f8887ace8013bccf789c460440706c6f72e14e4f292ddb7126c365bf04c65f121a69c1c778999530dd15cf2480110975122e451f7710616` |
| `Formula` | `sort-instance` | `blake3-512:78296b0cb631f8ab9f66d369ac768c64ef29840c10c0ad378d778222af5af7e2335c5c48526fe2b61e51f74034e523859250d1443aa0d7200af5c91329b038ea` |
| `Term` | `sort-instance` | `blake3-512:2b8bffc50e1a4fcae7e3954289941eb5811d14a6175420b06f71742e4ae9a9ce1a848a9251aec73f436123008fbdaf3d3c98d1c1fce04c3d89680051a7bdcafb` |
| `List<T>` | `sort-instance` | `blake3-512:e3f8d17445f9d2ce89c41c09cbeea08a8bc685d1c34a9fd3dfa7b1df17a94f40eab37396615501f1468baf2a1480fd5a27330ea23202b99876c5f4d97fa2cfb2` |
| `Map<K,V>` | `sort-instance` | `blake3-512:b81923e3273fedfce0b84d401d8b30965d4c72530af6c7538d9ed9b2905348fa3c639636b21b3f47ac8a242e79eef8e278b1d6c9cfab8e289cf059cef94c82e1` |
## Proc Macro Invocation Concept Shapes

`concept:proc-macro-invocation(macro_cid, args, token_stream)` carries procedural macro syntax without expansion.
`concept:derive-attribute(macro_cid, traits, token_stream)` is the typed Rust derive subcase.

| Concept | Shape CID | Notes |
| --- | --- | --- |
| `concept:proc-macro-invocation` | `blake3-512:b877e50648f55cf4622c303096bf51e4ac0bac8c8d851fa0a5d9921b2ed9b2513a7f6002293058ae489b7ef910bbfa1979077e0e2fe596cb2ac3296fe9fbf858` | carries macro CID, parsed args, and lossless source tokens |
| `concept:derive-attribute` | `blake3-512:a7e3cd707705197fdc7d8def39d4c075d36b292b5ad41343bd5939a82af4465f7a4baf0aaa1cbe56fe6523c7fea29940e22f06eddc9081ddac0372fd209e01d6` | typed derive attribute subcase with trait path terms |
