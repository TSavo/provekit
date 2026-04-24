CREATE TABLE `principle_matches` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`principle_name` text NOT NULL,
	`file_id` integer NOT NULL,
	`root_match_node_id` text NOT NULL,
	`severity` text NOT NULL,
	`message` text NOT NULL,
	FOREIGN KEY (`root_match_node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `principle_matches_by_principle_name` ON `principle_matches` (`principle_name`);--> statement-breakpoint
CREATE INDEX `principle_matches_by_file_id` ON `principle_matches` (`file_id`);--> statement-breakpoint
CREATE TABLE `principle_match_captures` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`match_id` integer NOT NULL,
	`capture_name` text NOT NULL,
	`captured_node_id` text NOT NULL,
	FOREIGN KEY (`match_id`) REFERENCES `principle_matches`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`captured_node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `principle_match_captures_by_match_id` ON `principle_match_captures` (`match_id`);
