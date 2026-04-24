CREATE TABLE `files` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`path` text NOT NULL,
	`content_hash` text NOT NULL,
	`parsed_at` integer NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `files_path_unique` ON `files` (`path`);--> statement-breakpoint
CREATE INDEX `files_by_content_hash` ON `files` (`content_hash`);--> statement-breakpoint
CREATE TABLE `nodes` (
	`id` text PRIMARY KEY NOT NULL,
	`file_id` integer NOT NULL,
	`source_start` integer NOT NULL,
	`source_end` integer NOT NULL,
	`source_line` integer NOT NULL,
	`source_col` integer NOT NULL,
	`subtree_hash` text NOT NULL,
	FOREIGN KEY (`file_id`) REFERENCES `files`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `nodes_by_file_id` ON `nodes` (`file_id`);--> statement-breakpoint
CREATE INDEX `nodes_by_subtree_hash` ON `nodes` (`subtree_hash`);--> statement-breakpoint
CREATE TABLE `node_children` (
	`parent_id` text NOT NULL,
	`child_id` text NOT NULL,
	`child_order` integer NOT NULL,
	PRIMARY KEY(`parent_id`, `child_id`),
	FOREIGN KEY (`parent_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`child_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `nc_by_child_id` ON `node_children` (`child_id`);