CREATE TABLE `harvest_expressibility` (
	`project` text NOT NULL,
	`bug_id` text NOT NULL,
	`tag` text NOT NULL,
	`layer1_recognized` integer NOT NULL,
	`layer1_matched_principles` text DEFAULT '[]' NOT NULL,
	`signature_columns` text DEFAULT '[]' NOT NULL,
	`signature_kinds` text DEFAULT '[]' NOT NULL,
	`signature_relations` text DEFAULT '[]' NOT NULL,
	`missing_columns` text DEFAULT '[]' NOT NULL,
	`missing_relations` text DEFAULT '[]' NOT NULL,
	`audit_line` text NOT NULL,
	`tagger_version` text NOT NULL,
	`tagged_at` text NOT NULL,
	PRIMARY KEY(`project`, `bug_id`)
);
--> statement-breakpoint
CREATE INDEX `harvest_expressibility_by_tag` ON `harvest_expressibility` (`tag`);--> statement-breakpoint
CREATE INDEX `harvest_expressibility_by_tagger_version` ON `harvest_expressibility` (`tagger_version`);
