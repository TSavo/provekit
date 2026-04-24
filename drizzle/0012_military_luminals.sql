CREATE TABLE `fix_bundles` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`bundle_type` text NOT NULL,
	`created_at` integer NOT NULL,
	`signal_rawtext` text NOT NULL,
	`signal_source` text NOT NULL,
	`signal_summary` text NOT NULL,
	`primary_layer` text NOT NULL,
	`locus_file` text NOT NULL,
	`locus_line` integer NOT NULL,
	`locus_primary_node` text,
	`applied_at` integer,
	`commit_sha` text,
	`confidence` real NOT NULL
);
--> statement-breakpoint
CREATE INDEX `fix_bundles_by_bundle_type` ON `fix_bundles` (`bundle_type`);--> statement-breakpoint
CREATE INDEX `fix_bundles_by_primary_layer` ON `fix_bundles` (`primary_layer`);--> statement-breakpoint
CREATE TABLE `fix_bundle_artifacts` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`bundle_id` integer NOT NULL,
	`kind` text NOT NULL,
	`payload_json` text NOT NULL,
	`passed_oracles` text NOT NULL,
	`verified_at` integer NOT NULL,
	FOREIGN KEY (`bundle_id`) REFERENCES `fix_bundles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `fix_bundle_artifacts_by_bundle_id` ON `fix_bundle_artifacts` (`bundle_id`);--> statement-breakpoint
CREATE INDEX `fix_bundle_artifacts_by_kind_bundle_id` ON `fix_bundle_artifacts` (`kind`,`bundle_id`);--> statement-breakpoint
CREATE TABLE `llm_calls` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`bundle_id` integer NOT NULL,
	`stage` text NOT NULL,
	`model_tier` text NOT NULL,
	`prompt` text NOT NULL,
	`response` text NOT NULL,
	`seed` integer,
	`ms` integer NOT NULL,
	`called_at` integer NOT NULL,
	FOREIGN KEY (`bundle_id`) REFERENCES `fix_bundles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `llm_calls_by_bundle_id_stage` ON `llm_calls` (`bundle_id`,`stage`);--> statement-breakpoint
CREATE TABLE `pending_fixes` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`source_bundle_id` integer NOT NULL,
	`site_node_id` text NOT NULL,
	`site_file` text NOT NULL,
	`site_line` integer NOT NULL,
	`reason` text NOT NULL,
	`priority` integer NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`source_bundle_id`) REFERENCES `fix_bundles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `pending_fixes_by_source_bundle_id` ON `pending_fixes` (`source_bundle_id`);--> statement-breakpoint
CREATE INDEX `pending_fixes_by_priority` ON `pending_fixes` (`priority`);
