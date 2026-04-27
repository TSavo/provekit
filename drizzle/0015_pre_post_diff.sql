CREATE TABLE `pre_post_diff` (
	`context` text NOT NULL,
	`file_path` text NOT NULL,
	`change_kind` text NOT NULL,
	`pre_fingerprint` text,
	`pre_parent_fingerprint` text,
	`pre_ordinal` integer,
	`pre_kind` text,
	`pre_line` integer,
	`pre_col` integer,
	`pre_start` integer,
	`pre_end` integer,
	`pre_text_preview` text,
	`post_fingerprint` text,
	`post_parent_fingerprint` text,
	`post_ordinal` integer,
	`post_kind` text,
	`post_line` integer,
	`post_col` integer,
	`post_start` integer,
	`post_end` integer,
	`post_text_preview` text
);
--> statement-breakpoint
CREATE INDEX `pre_post_diff_by_context` ON `pre_post_diff` (`context`);--> statement-breakpoint
CREATE INDEX `pre_post_diff_by_context_change` ON `pre_post_diff` (`context`,`change_kind`);--> statement-breakpoint
CREATE INDEX `pre_post_diff_by_post_location` ON `pre_post_diff` (`context`,`file_path`,`post_start`,`post_kind`);