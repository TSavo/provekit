CREATE TABLE `gap_reports` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`clause_id` integer NOT NULL,
	`trace_id` integer,
	`kind` text NOT NULL,
	`smt_constant` text,
	`at_node_ref` text,
	`smt_value_id` integer,
	`runtime_value_id` integer,
	`explanation` text,
	FOREIGN KEY (`clause_id`) REFERENCES `clauses`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`trace_id`) REFERENCES `traces`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`smt_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`runtime_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `gr_by_clause` ON `gap_reports` (`clause_id`);--> statement-breakpoint
CREATE INDEX `gr_by_kind` ON `gap_reports` (`kind`);--> statement-breakpoint
CREATE INDEX `gr_by_node_ref` ON `gap_reports` (`at_node_ref`);