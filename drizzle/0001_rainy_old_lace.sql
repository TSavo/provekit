CREATE TABLE `trace_values` (
	`trace_id` integer NOT NULL,
	`node_id` text NOT NULL,
	`iteration_index` integer,
	`root_value_id` integer NOT NULL,
	PRIMARY KEY(`trace_id`, `node_id`, `iteration_index`),
	FOREIGN KEY (`trace_id`) REFERENCES `traces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`root_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `tv_by_node` ON `trace_values` (`node_id`);--> statement-breakpoint
CREATE INDEX `tv_by_root` ON `trace_values` (`root_value_id`);--> statement-breakpoint
CREATE TABLE `traces` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`clause_id` integer NOT NULL,
	`captured_at` integer NOT NULL,
	`outcome_kind` text NOT NULL,
	`outcome_value_id` integer,
	`untestable_reason` text,
	`inputs_hash` text NOT NULL,
	FOREIGN KEY (`outcome_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `traces_by_clause` ON `traces` (`clause_id`);