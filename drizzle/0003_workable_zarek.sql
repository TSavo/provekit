CREATE TABLE `clause_bindings` (
	`clause_id` integer NOT NULL,
	`smt_constant` text NOT NULL,
	`source_line` integer NOT NULL,
	`source_expr` text NOT NULL,
	`sort` text NOT NULL,
	PRIMARY KEY(`clause_id`, `smt_constant`),
	FOREIGN KEY (`clause_id`) REFERENCES `clauses`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `clause_witnesses` (
	`clause_id` integer NOT NULL,
	`smt_constant` text NOT NULL,
	`model_value_id` integer NOT NULL,
	`sort` text NOT NULL,
	PRIMARY KEY(`clause_id`, `smt_constant`),
	FOREIGN KEY (`clause_id`) REFERENCES `clauses`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`model_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `cw_by_value` ON `clause_witnesses` (`model_value_id`);--> statement-breakpoint
CREATE TABLE `clauses` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`contract_key` text NOT NULL,
	`verdict` text NOT NULL,
	`smt2` text NOT NULL,
	`clause_hash` text NOT NULL,
	`principle_name` text,
	`complexity` integer,
	`confidence` text,
	`judge_note` text,
	`vacuous_reason` text
);
--> statement-breakpoint
CREATE INDEX `clauses_by_contract` ON `clauses` (`contract_key`);--> statement-breakpoint
CREATE INDEX `clauses_by_hash` ON `clauses` (`clause_hash`);