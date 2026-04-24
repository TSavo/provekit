CREATE TABLE `principles_library` (
	`name` text PRIMARY KEY NOT NULL,
	`dsl_path` text NOT NULL,
	`json_path` text NOT NULL,
	`confidence_tier` text NOT NULL,
	`added_bundle_id` integer,
	`added_at` integer NOT NULL,
	`false_negative_count` integer DEFAULT 0 NOT NULL,
	`successful_application_count` integer DEFAULT 0 NOT NULL,
	FOREIGN KEY (`added_bundle_id`) REFERENCES `fix_bundles`(`id`) ON UPDATE no action ON DELETE set null
);
--> statement-breakpoint
CREATE INDEX `principles_library_by_confidence_tier` ON `principles_library` (`confidence_tier`);--> statement-breakpoint
CREATE INDEX `principles_library_by_added_bundle_id` ON `principles_library` (`added_bundle_id`);
