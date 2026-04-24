CREATE TABLE `runtime_value_array_elements` (
	`parent_value_id` integer NOT NULL,
	`element_index` integer NOT NULL,
	`child_value_id` integer NOT NULL,
	PRIMARY KEY(`parent_value_id`, `element_index`),
	FOREIGN KEY (`parent_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`child_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `rvae_by_child` ON `runtime_value_array_elements` (`child_value_id`);--> statement-breakpoint
CREATE TABLE `runtime_value_object_members` (
	`parent_value_id` integer NOT NULL,
	`key` text NOT NULL,
	`child_value_id` integer NOT NULL,
	PRIMARY KEY(`parent_value_id`, `key`),
	FOREIGN KEY (`parent_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`child_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `rvom_by_child` ON `runtime_value_object_members` (`child_value_id`);--> statement-breakpoint
CREATE TABLE `runtime_values` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`kind` text NOT NULL,
	`number_value` real,
	`string_value` text,
	`bool_value` integer,
	`circular_target_id` integer,
	`truncation_note` text,
	FOREIGN KEY (`circular_target_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `rv_by_kind` ON `runtime_values` (`kind`);--> statement-breakpoint
CREATE INDEX `rv_by_kind_number` ON `runtime_values` (`kind`,`number_value`);--> statement-breakpoint
CREATE INDEX `rv_by_kind_string` ON `runtime_values` (`kind`,`string_value`);--> statement-breakpoint
CREATE INDEX `rv_by_kind_bool` ON `runtime_values` (`kind`,`bool_value`);