CREATE TABLE `node_arithmetic` (
	`node_id` text PRIMARY KEY NOT NULL,
	`op` text NOT NULL,
	`lhs_node` text NOT NULL,
	`rhs_node` text NOT NULL,
	`result_sort` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`lhs_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`rhs_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_arithmetic_by_op_node_id` ON `node_arithmetic` (`op`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_assigns` (
	`node_id` text PRIMARY KEY NOT NULL,
	`target_node` text NOT NULL,
	`rhs_node` text,
	`assign_kind` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`target_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`rhs_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_assigns_by_assign_kind_node_id` ON `node_assigns` (`assign_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_returns` (
	`node_id` text PRIMARY KEY NOT NULL,
	`exit_kind` text NOT NULL,
	`value_node` text,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`value_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_returns_by_exit_kind_node_id` ON `node_returns` (`exit_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_member_access` (
	`node_id` text PRIMARY KEY NOT NULL,
	`object_node` text NOT NULL,
	`property_name` text,
	`computed` integer NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`object_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_member_access_by_property_name_object_node` ON `node_member_access` (`property_name`,`object_node`);--> statement-breakpoint
CREATE TABLE `node_non_null_assertion` (
	`node_id` text PRIMARY KEY NOT NULL,
	`operand_node` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`operand_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_truthiness` (
	`node_id` text PRIMARY KEY NOT NULL,
	`coercion_kind` text NOT NULL,
	`operand_node` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`operand_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_truthiness_by_coercion_kind_node_id` ON `node_truthiness` (`coercion_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_narrows` (
	`node_id` text PRIMARY KEY NOT NULL,
	`target_node` text NOT NULL,
	`narrowing_kind` text NOT NULL,
	`narrowed_type` text,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`target_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_narrows_by_narrowing_kind_node_id` ON `node_narrows` (`narrowing_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_decides` (
	`node_id` text PRIMARY KEY NOT NULL,
	`condition_node` text NOT NULL,
	`consequent_node` text,
	`alternate_node` text,
	`decision_kind` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`condition_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`consequent_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`alternate_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_iterates` (
	`node_id` text PRIMARY KEY NOT NULL,
	`init_node` text,
	`condition_node` text,
	`update_node` text,
	`body_node` text NOT NULL,
	`loop_kind` text NOT NULL,
	`executes_at_least_once` integer NOT NULL,
	`collection_source_node` text,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`init_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`condition_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`update_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`body_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`collection_source_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_yields` (
	`node_id` text PRIMARY KEY NOT NULL,
	`yield_kind` text NOT NULL,
	`source_call_node` text,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`source_call_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_throws` (
	`node_id` text PRIMARY KEY NOT NULL,
	`handler_node` text,
	`is_inside_handler` integer NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`handler_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_calls` (
	`node_id` text PRIMARY KEY NOT NULL,
	`callee_node` text NOT NULL,
	`callee_name` text,
	`arg_count` integer NOT NULL,
	`is_method_call` integer NOT NULL,
	`callee_is_async` integer NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`callee_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_calls_by_callee_name_node_id` ON `node_calls` (`callee_name`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_captures` (
	`node_id` text NOT NULL,
	`captured_name` text NOT NULL,
	`declared_in_node` text,
	`mutable` integer NOT NULL,
	PRIMARY KEY(`node_id`, `captured_name`),
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`declared_in_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_captures_by_captured_name` ON `node_captures` (`captured_name`);--> statement-breakpoint
CREATE TABLE `node_pattern` (
	`node_id` text PRIMARY KEY NOT NULL,
	`pattern_kind` text NOT NULL,
	`slot_key` text,
	`rename_to` text,
	`default_smt` text,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `node_binding` (
	`node_id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`declared_type` text,
	`binding_kind` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_binding_by_name` ON `node_binding` (`name`);--> statement-breakpoint
CREATE INDEX `node_binding_by_binding_kind_node_id` ON `node_binding` (`binding_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `node_signal` (
	`node_id` text PRIMARY KEY NOT NULL,
	`signal_kind` text NOT NULL,
	`signal_payload` text NOT NULL,
	FOREIGN KEY (`node_id`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `node_signal_by_signal_kind_node_id` ON `node_signal` (`signal_kind`,`node_id`);--> statement-breakpoint
CREATE TABLE `signal_interpolations` (
	`signal_node` text NOT NULL,
	`slot_index` integer NOT NULL,
	`interpolated_node` text NOT NULL,
	PRIMARY KEY(`signal_node`, `slot_index`),
	FOREIGN KEY (`signal_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`interpolated_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `signal_interpolations_by_interpolated_node` ON `signal_interpolations` (`interpolated_node`);--> statement-breakpoint
ALTER TABLE `nodes` ADD `kind` text NOT NULL;