use crate::*;

    #[test]
    fn test_post_process_prisma_missing_url() {
        let content = "datasource db {\n  provider = \"postgresql\"\n}\n\nmodel User {\n  id            String     @id\n  userId        String\n  user          User       @relation(fields: [userId], references: [User.id])\n}";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("prisma/schema.prisma", content, &metamanifest, false);
        assert!(processed.contains("url      = env(\"DATABASE_URL\")"));
        assert!(processed.contains("references: [id]"));
        assert!(processed.contains("userId        String\n"));
    }


    #[test]
    fn test_post_process_prisma_default_export() {
        let content = "import { PrismaClient } from '@prisma/client';\nexport const prisma = new PrismaClient();";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/lib/prisma.ts", content, &metamanifest, false);
        assert!(processed.contains("export default prisma;"));
    }


    #[test]
    fn test_prisma_sanitizer_fixes_relation_on_scalar_and_nextauth() {
        // Exact schema that broke the "Health App" project
        let content = "generator client {\n  provider = \"prisma-client-js\"\n}\n\ndatasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n\nmodel User {\n  id            String     @id @default(cuid())\n  createdAt     DateTime   @default(now())\n  email         String     @unique\n  name          String?\n  accounts      Account[]\n  sessions      Session[]\n}\n\nmodel Account {\n  id                String     @id @default(cuid())\n  type              String\n  provider          String\n  providerAccountId String\n  refresh_token     String?\n  access_token      String?\n  expires_at        Int?\n  token_type        String?\n  scope             String?\n  id_token          String?\n  session_state     String?\n  userId            String     @relation(fields: [userId], references: [id], onDelete: Cascade)\n  user              User       @relation(fields: [userId], references: [id], onDelete: Cascade)\n}\n\nmodel Session {\n  id           String   @id @default(cuid())\n  sessionToken String   @unique\n  expires      DateTime\n  userId       String   \n  user         User     @relation(fields: [userId], references: [id], onDelete: Cascade)\n}";
        let processed = post_process_generated_file("prisma/schema.prisma", content, &serde_json::json!({}), false);

        // No @relation on a scalar field anymore
        for line in processed.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == "userId" {
                assert!(!line.contains("@relation"), "scalar userId still has @relation: {}", line);
            }
        }
        // Canonical NextAuth constraint added
        assert!(processed.contains("@@unique([provider, providerAccountId])"));
        // Back-relations are not duplicated
        assert_eq!(processed.matches("Account[]").count(), 1);
        assert_eq!(processed.matches("Session[]").count(), 1);
    }


    #[test]
    fn test_nextauth_template_matches_int_user_id_and_adds_back_relations() {
        let content = "model User {\n  id    Int    @id @default(autoincrement())\n  email String @unique\n\n  @@index([email])\n}\n\nmodel Account {\n  id                String @id\n  userId            Int\n  provider          String\n  providerAccountId String\n}\n\nmodel Session {\n  id           String @id\n  sessionToken String @unique\n  userId       Int\n  expires      DateTime\n}";
        let processed = apply_nextauth_prisma_template(content);
        assert!(processed.contains("  userId            Int\n"));
        assert!(processed.contains("  userId       Int\n"));
        assert!(processed.contains("accounts      Account[]"));
        assert!(processed.contains("sessions      Session[]"));
        // Back-relations are inserted before the @@index block attribute
        let acc_idx = processed.find("accounts      Account[]").unwrap();
        let idx_idx = processed.find("@@index([email])").unwrap();
        assert!(acc_idx < idx_idx);
    }


    #[test]
    fn test_nextauth_template_skips_non_nextauth_models() {
        let content = "model User {\n  id String @id\n}\n\nmodel Session {\n  id      String @id\n  workout String\n}";
        assert_eq!(apply_nextauth_prisma_template(content), content);
    }



    #[test]
    fn test_ensure_prisma_opposite_relations() {
        let broken = r#"
model User {
  id    String @id @default(cuid())
  email String @unique
}

model Project {
  id   String @id @default(cuid())
  name String
}

model Team {
  id        String  @id @default(cuid())
  userId    String
  member    User    @relation(fields: [userId], references: [id], name: "TeamMember")
  projectId String
  project   Project @relation(fields: [projectId], references: [id])
}

model Analytics {
  id        String   @id @default(cuid())
  timestamp DateTime @default(now())
  projectId String?
  project   Project? @relation(fields: [projectId], references: [id])
}

model Dashboard {
  id       String @id @default(cuid())
  settings Json
  userId   String
  user     User   @relation(fields: [userId], references: [id])
}
"#;

        let fixed = post_process_generated_file("prisma/schema.prisma", broken, &serde_json::json!({}), false);
        // Opposite relation with relation name preserved
        assert!(fixed.contains("Team[]") && fixed.contains("@relation(\"TeamMember\")"));
        // Opposites for Dashboard and Analytics added
        assert!(fixed.contains("Dashboard[]"));
        assert!(fixed.contains("Analytics[]"));
        assert!(fixed.contains("Team[]"));
    }


    #[test]
    fn test_offline_package_sanitizer_and_prisma_fk_healer() {
        let mut pkg_json = serde_json::json!({
            "name": "skillmatch",
            "dependencies": {
                "next": "14.2.35",
                "fi": "*",
                "typescript": "^4.5.2"
            },
            "devDependencies": {
                "typescript": "^5.3.3",
                "@typescript-eslint/parser": "^4.29.2",
                "@typescript-eslint/eslint-plugin": "^4.29.2",
                "eslint-config-next": "14.2.35",
                "eslint": "^8.57.0"
            }
        });
        assert!(sanitize_package_json_value_for_offline(&mut pkg_json));
        let deps = pkg_json["dependencies"].as_object().unwrap();
        let dev_deps = pkg_json["devDependencies"].as_object().unwrap();
        assert!(!deps.contains_key("fi"));
        assert!(deps.contains_key("react-icons"));
        assert!(!deps.contains_key("typescript"));
        assert!(dev_deps.contains_key("typescript"));
        assert!(!dev_deps.contains_key("@typescript-eslint/parser"));
        assert!(!dev_deps.contains_key("eslint-config-next"));

        let broken_prisma = r#"datasource db {
  provider = "sqlite"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model UserProfile {
  id   String @id @default(cuid())
  name String
}

model Payment {
  id          String      @id @default(cuid())
  amount      Float
  userProfile UserProfile @relation(fields: [userProfileId], references: [id])
}
"#;
        let fixed_prisma = post_process_generated_file("prisma/schema.prisma", broken_prisma, &serde_json::json!({}), false);
        assert!(fixed_prisma.contains("userProfileId"));
        assert!(fixed_prisma.contains("payments"));

        let partial_ai_snippet = r#"model Payment {
  id            String      @id @default(cuid())
  amount        Float
  userProfileId String
  userProfile   UserProfile @relation(fields: [userProfileId], references: [id])
}"#;
        let merged = merge_prisma_ai_response(&fixed_prisma, partial_ai_snippet);
        assert!(merged.contains("datasource db"));
        assert!(merged.contains("generator client"));
        assert!(merged.contains("model UserProfile"));
        assert!(merged.contains("model Payment"));
    }


    #[test]
    fn test_sanitize_prisma_relations_removes_ghosts_and_enforces_unique() {
        let input_schema = r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id                String         @id @default(cuid())
  subscriptions     Subscription[] @relation("UserSubscriptions")
  userSubscriptions Subscription[] @relation("UserSubscriptionsSecondary")
}

model Project {
  id                        String      @id @default(cuid())
  projectAnalytics          Analytics?  @relation("ProjectAnalytics")
  projectAnalyticsSecondary Analytics[] @relation("ProjectAnalyticsSecondary")
}

model Subscription {
  id                String         @id @default(cuid())
  userId            String
  user              User           @relation("UserSubscriptions", fields: [userId], references: [id])
  userSubscriptions Subscription[] @relation("UserSubscriptionsSecondary")
}

model Analytics {
  id                        String      @id @default(cuid())
  projectId                 String
  project                   Project     @relation("ProjectAnalytics", fields: [projectId], references: [id])
  projectAnalyticsSecondary Analytics[] @relation("ProjectAnalyticsSecondary")
}
"#;
        let sanitized = sanitize_prisma_relations(input_schema);
        
        // 1. Ghost duplicate relation on User is pruned
        assert!(!sanitized.contains("UserSubscriptionsSecondary"));
        assert!(!sanitized.contains("userSubscriptions"));
        assert!(sanitized.contains("subscriptions     Subscription[] @relation(\"UserSubscriptions\")"));

        // 2. Ghost self-relation on Subscription is pruned
        assert!(!sanitized.contains("Subscription[] @relation(\"UserSubscriptionsSecondary\")"));

        // 3. Ghost duplicate and self relations on Project & Analytics are pruned
        assert!(!sanitized.contains("ProjectAnalyticsSecondary"));
        assert!(!sanitized.contains("projectAnalyticsSecondary"));

        // 4. 1-to-1 relation foreign key on Analytics gets @unique
        assert!(sanitized.contains("projectId                 String @unique"));
    }


    #[test]
    fn test_api_route_cot_and_prisma_singleton_scope() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_api_cot_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(temp_dir.join(".axiom"));

        // 1. Verify src/lib/prisma.ts gets singleton scope
        let prisma_lib_prompt = get_expert_system_prompt("src/lib/prisma.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(prisma_lib_prompt.contains("PRISMA SINGLETON RULES:"));
        assert!(prisma_lib_prompt.contains("globalForPrisma.prisma ??"));
        assert!(prisma_lib_prompt.contains("export default prisma;"));

        // 2. Populate live_builder_guide with harvested database models
        let lbg = serde_json::json!({
            "projectSpecification": { "category": "edtech" },
            "sharedContracts": {
                "database": {
                    "models": {
                        "Course": {
                            "fields": ["id", "title", "price"],
                            "relations": ["lessons: Lesson[]"]
                        }
                    },
                    "enums": {
                        "CourseStatus": ["DRAFT", "PUBLISHED"]
                    }
                },
                "api": { "endpoints": {} },
                "hooks": {}
            }
        });
        std::fs::write(temp_dir.join(".axiom/live_builder_guide.json"), serde_json::to_string_pretty(&lbg).unwrap()).unwrap();

        // 3. Verify API route prompt gets CoT, domain archetype, and active models
        let api_prompt = get_expert_system_prompt("src/app/api/courses/route.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(api_prompt.contains("API DOMAIN ARCHETYPE: EdTech, HealthCare & Progression Tracking"));
        assert!(api_prompt.contains("BACKEND API GENERATION CHAIN-OF-THOUGHT"));
        assert!(api_prompt.contains("Step 1 [Imports & Core Bindings]"));
        assert!(api_prompt.contains("Step 2 [Schema Validation & Safe Defaults]"));
        assert!(api_prompt.contains("Step 3 [Typed Relational Queries]"));
        assert!(api_prompt.contains("Step 4 [Local Resilience & Status Gate]"));
        assert!(api_prompt.contains("ACTIVE DATABASE MODELS:"));
        assert!(api_prompt.contains("model Course"));
        assert!(api_prompt.contains("scalars: [id, title, price]"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_disambiguate_prisma_ambiguous_relations() {
        let broken_schema = r#"
datasource db {
  provider = "sqlite"
  url      = "file:./dev.db"
}

generator client {
  provider = "prisma-client-js"
}

model Transaction {
  id                  String   @id @default(cuid())
  amount              Float
  sourceWalletId      String
  sourceWallet        Wallet   @relation(fields: [sourceWalletId], references: [id])
  destinationWalletId String?
  destinationWallet   Wallet?  @relation(fields: [destinationWalletId], references: [id])
}

model Wallet {
  id        String   @id @default(cuid())
  balance   Float
}
"#;

        let sanitized = post_process_generated_file("prisma/schema.prisma", broken_schema, &serde_json::json!({}), false);
        // Verify both sides received explicit relation names
        assert!(sanitized.contains("@relation(\"TransactionSourceWallet\""));
        assert!(sanitized.contains("@relation(\"TransactionDestinationWallet\""));
        // Verify Wallet received named back-relations
        assert!(sanitized.contains("sourceWalletTransactions"));
        assert!(sanitized.contains("destinationWalletTransactions"));
        // Verify ambiguous collision field `transactionsTransaction` was NEVER generated
        assert!(!sanitized.contains("transactionsTransaction"));
    }


    #[test]
    fn test_disambiguate_prisma_self_relations_and_mutual_relations() {
        let self_rel_schema = r#"
model Category {
  id        String     @id @default(cuid())
  parentId  String?
  parent    Category?  @relation(fields: [parentId], references: [id])
  children  Category[]
}
"#;
        let sanitized = post_process_generated_file("prisma/schema.prisma", self_rel_schema, &serde_json::json!({}), false);
        // Verify self-relation hierarchy receives relation name
        assert!(sanitized.contains("@relation(\"CategoryHierarchy\""));

        // Mutual relations test
        let mutual_schema = r#"
model User {
  id          String   @id @default(cuid())
  companyId   String
  company     Company  @relation(fields: [companyId], references: [id])
}

model Company {
  id          String   @id @default(cuid())
  ownerId     String
  owner       User     @relation(fields: [ownerId], references: [id])
}
"#;
        let sanitized_mutual = post_process_generated_file("prisma/schema.prisma", mutual_schema, &serde_json::json!({}), false);
        assert!(sanitized_mutual.contains("@relation(\"UserCompany\""));
        assert!(sanitized_mutual.contains("@relation(\"CompanyOwner\""));
    }


    #[test]
    fn test_prisma_referential_actions_and_carbonchain_assigned_to_healing() {
        // 1. Referential action normalization inside @relation(...) without mutating enum/scalar names
        let raw_actions = r#"
enum ActionType {
  NO_ACTION
}

model Item {
  id       String  @id @default(cuid())
  parentId String?
  action   ActionType @default(NO_ACTION)
  parent   Item?   @relation(fields: [parentId], references: [id], onDelete: SET_NULL, onUpdate: CASCADE)
  otherId  String?
  other    Other?  @relation(fields: [otherId], references: [id], onDelete: NO_ACTION, onUpdate: SET_DEFAULT)
  // onDelete: SET_NULL comment must remain untouched
}
"#;
        let normalized = normalize_prisma_referential_actions(raw_actions);
        assert!(normalized.contains("onDelete: SetNull, onUpdate: Cascade"));
        assert!(normalized.contains("onDelete: NoAction, onUpdate: SetDefault"));
        assert!(normalized.contains("NO_ACTION\n"));
        assert!(normalized.contains("@default(NO_ACTION)"));
        assert!(normalized.contains("// onDelete: SET_NULL comment must remain untouched"));

        // 2. extract_prisma_relation_name must ignore map: "..." and extract positional or name: "..."
        assert_eq!(
            extract_prisma_relation_name("  user User @relation(fields: [userId], references: [id], map: \"fk_user\")"),
            None
        );
        assert_eq!(
            extract_prisma_relation_name("  assignedTo User? @relation(fields: [assignedToId], references: [id], onDelete: SetNull, name: \"AssignedTo\")"),
            Some("AssignedTo".to_string())
        );

        // 3. CarbonChain Check 3 scenario (SET_NULL + explicit name: "AssignedTo" + un-named User.tasks)
        let check3_schema = r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id           String        @id @default(cuid())
  email        String        @unique
  tasks        Task[]
}

model Task {
  id           String   @id @default(cuid())
  assignedToId String?
  assignedTo   User?    @relation(fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")
}
"#;
        let fixed_check3 = post_process_generated_file("prisma/schema.prisma", check3_schema, &serde_json::json!({}), false);
        assert!(fixed_check3.contains("onDelete: SetNull"), "SET_NULL should become SetNull:\n{}", fixed_check3);
        assert!(!fixed_check3.contains("SET_NULL"));
        assert!(!fixed_check3.contains("tasksTask"), "must NOT synthesize duplicate tasksTask:\n{}", fixed_check3);
        assert!(fixed_check3.contains("tasks        Task[] @relation(\"AssignedTo\")"), "User.tasks should receive @relation(\"AssignedTo\"):\n{}", fixed_check3);

        // 4. CarbonChain Check 4 full corrupted schema (both `tasks` and `tasksTask` + dual Project/Analytics + TeamProjects M2M)
        let check4_schema = r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id            String   @id @default(cuid())
  email         String   @unique
  projects      Project[]
  teams         Team[]
  tasks        Task[] @relation("AssignedTo")
  subscriptions Subscription[]
  tasksTask    Task[] @relation("AssignedTo")
}

model Team {
  id        String   @id @default(cuid())
  name      String
  users     User[]
  projects  Project[] @relation("TeamProjects")
  subscriptions Subscription[]
}

model Project {
  id          String   @id @default(cuid())
  ownerId     String
  owner       User     @relation(fields: [ownerId], references: [id])
  tasks       Task[]
  analytics   Analytics? @relation("ProjectAnalytics", fields: [analyticsId], references: [id])
  analyticsId String?  @unique
  projectAnalytics Analytics? @relation("AnalyticsProject")
  teams       Team[] @relation("TeamProjects")
}

model Task {
  id           String   @id @default(cuid())
  projectId    String
  project      Project  @relation(fields: [projectId], references: [id])
  assignedTo   User?    @relation(fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")
  assignedToId String?
}

model Subscription {
  id     String  @id @default(cuid())
  userId String?
  teamId String?
  user   User?   @relation(fields: [userId], references: [id])
  team   Team?   @relation(fields: [teamId], references: [id])
}

model Analytics {
  id               String   @id @default(cuid())
  projectId        String   @unique
  project          Project  @relation("AnalyticsProject", fields: [projectId], references: [id])
  analyticsProject Project? @relation("ProjectAnalytics")
}
"#;
        let healed_check4 = post_process_generated_file("prisma/schema.prisma", check4_schema, &serde_json::json!({}), false);
        assert!(!healed_check4.contains("tasksTask"), "duplicate tasksTask must be pruned:\n{}", healed_check4);
        assert!(healed_check4.contains("tasks        Task[] @relation(\"AssignedTo\")"));
        assert!(!healed_check4.contains("SET_NULL"));
        assert!(healed_check4.contains("onDelete: SetNull"));
        assert!(healed_check4.contains("projects  Project[] @relation(\"TeamProjects\")"));
        assert!(healed_check4.contains("teams       Team[] @relation(\"TeamProjects\")"));
        assert!(healed_check4.contains("projectAnalytics Analytics? @relation(\"AnalyticsProject\")"));
        assert!(healed_check4.contains("analyticsProject Project? @relation(\"ProjectAnalytics\")"));

        // 4b. Even if `tasksTask` precedes `tasks` in User, natural `tasks` wins over synthetic `tasksTask`
        let reversed_schema = check4_schema
            .replace("  tasks        Task[] @relation(\"AssignedTo\")\n", "")
            .replace("  tasksTask    Task[] @relation(\"AssignedTo\")", "  tasksTask    Task[] @relation(\"AssignedTo\")\n  tasks        Task[] @relation(\"AssignedTo\")");
        let healed_reversed = post_process_generated_file("prisma/schema.prisma", &reversed_schema, &serde_json::json!({}), false);
        assert!(!healed_reversed.contains("tasksTask"));
        assert!(healed_reversed.contains("tasks        Task[] @relation(\"AssignedTo\")"));

        // 5. Self-relation paired child field must survive sanitize_prisma_relations AND full pipeline
        let self_rel_with_set_null = r#"
model Category {
  id       String     @id @default(cuid())
  parentId String?
  parent   Category?  @relation("CategoryHierarchy", fields: [parentId], references: [id], onDelete: SET_NULL)
  children Category[] @relation("CategoryHierarchy")
}
"#;
        let healed_self = post_process_generated_file("prisma/schema.prisma", self_rel_with_set_null, &serde_json::json!({}), false);
        assert!(healed_self.contains("onDelete: SetNull"));
        assert!(healed_self.contains("children Category[] @relation(\"CategoryHierarchy\")"), "paired children self-relation was wrongly deleted:\n{}", healed_self);
    }


    #[test]
    fn test_prisma_quoted_parens_utf8_and_inline_comment_unique() {
        // Parentheses inside map: "..." must not truncate @relation(...) parsing
        let rel_line = r#"  assignedTo User? @relation(map: "fk(assigned_1)", fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")"#;
        assert_eq!(extract_prisma_relation_name(rel_line), Some("AssignedTo".to_string()));
        let norm_line = normalize_prisma_referential_actions(rel_line);
        assert!(norm_line.contains("onDelete: SetNull"), "Must normalize past quoted parens:\n{}", norm_line);

        // Multi-byte UTF-8 inline comment + scalar @relation must not panic or corrupt bytes
        let utf8_scalar = "model Audit {\n  članId String @relation(fields: [x], references: [id]) // čćžšđ komentar\n}\n";
        let fixed_utf8 = post_process_generated_file("prisma/schema.prisma", utf8_scalar, &serde_json::json!({}), false);
        assert!(!fixed_utf8.contains("@relation"));
        assert!(fixed_utf8.contains("// čćžšđ komentar"));

        // 1-to-1 @unique injection and back-relation @relation("...") attachment must preserve trailing inline comments
        let one_to_one_comment = r#"
model User {
  id    String @id @default(cuid())
  tasks Task[] // dodeljeni zadaci
}

model Task {
  id           String  @id @default(cuid())
  assignedToId String?
  assignedTo   User?   @relation(name: "AssignedTo", fields: [assignedToId], references: [id])
}

model Project {
  id        String     @id @default(cuid())
  analytics Analytics? @relation("ProjectAnalytics")
}

model Analytics {
  id        String  @id @default(cuid())
  projectId String  // FK ka projektu
  project   Project @relation("ProjectAnalytics", fields: [projectId], references: [id])
}
"#;
        let fixed_unique = sanitize_prisma_relations(one_to_one_comment);
        assert!(
            fixed_unique.contains("projectId String @unique // FK ka projektu"),
            "@unique must be inserted before inline comment:\n{}",
            fixed_unique
        );
        assert!(
            fixed_unique.contains("tasks Task[] @relation(\"AssignedTo\") // dodeljeni zadaci"),
            "@relation must be inserted before inline comment:\n{}",
            fixed_unique
        );
    }


    #[test]
    fn test_katastar_prisma_preamble_local_import_and_npm_errors_v47() {
        // 1. Prisma CLI preambles must be stripped so real P1012 error produces clean signature
        let prisma_raw = "Environment variables loaded from .env\nPrisma schema loaded from prisma\\schema.prisma\nDatasource \"db\": PostgreSQL\nError code: P1012\nerror: Error parsing attribute \"@relation\": Invalid referential action: `SET_NULL`";
        let prisma_sig = crate::katastar::normalize_error_signature("prisma", prisma_raw, Some("prisma/schema.prisma"));
        assert_eq!(prisma_sig.category, "prisma");
        assert_eq!(prisma_sig.error_code.as_deref(), Some("P1012"));
        assert!(prisma_sig.error_signature.starts_with("prisma:P1012:"));
        assert!(!prisma_sig.normalized_sample.contains("Environment variables loaded"));
        assert!(!prisma_sig.normalized_sample.contains("Prisma schema loaded"));

        // 2. Local import specifiers produce canonical :local-import signature
        let ts_local = crate::katastar::normalize_error_signature(
            "",
            "TS2307: Cannot find module 'src/models/MaintenanceAlert' or its corresponding type declarations.",
            Some("src/hooks/useMaintenanceAlerts.ts"),
        );
        assert_eq!(ts_local.category, "compile");
        assert_eq!(ts_local.error_code.as_deref(), Some("TS2307"));
        assert_eq!(ts_local.error_signature, "compile:TS2307:local-import");

        let wp_local = crate::katastar::normalize_error_signature(
            "",
            "Module not found: Can't resolve 'app/components/Missing'",
            Some("src/app/page.tsx"),
        );
        assert_eq!(wp_local.error_signature, "compile:MODULE_NOT_FOUND:local-import");

        // 3. npm ENOENT / ENOTFOUND / missing package normalization
        let enoent_sig = crate::katastar::normalize_error_signature(
            "npm",
            "npm error code ENOENT\nmissing package 'foo'",
            None,
        );
        assert_eq!(enoent_sig.error_signature, "npm:ENOENT:foo");

        let net_sig = crate::katastar::normalize_error_signature(
            "npm",
            "npm error code ENOTFOUND\nnpm error request to https://registry.npmjs.org/zod failed",
            None,
        );
        assert_eq!(net_sig.error_signature, "npm:ENOTFOUND:zod");
    }


    #[test]
    fn test_sanitizer_tsconfig_client_prisma_css_exports_and_ui_v47() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_san_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/dashboard")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();

        // 1. ensure_valid_tsconfig adds baseUrl and @/* -> ["./src/*"]
        std::fs::write(
            temp_dir.join("tsconfig.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "compilerOptions": { "strict": true }
            }))
            .unwrap(),
        )
        .unwrap();
        crate::sanitizer::ensure_valid_tsconfig(&temp_dir);
        let ts_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("tsconfig.json")).unwrap()).unwrap();
        assert_eq!(ts_val["compilerOptions"]["baseUrl"], ".");
        assert_eq!(ts_val["compilerOptions"]["paths"]["@/*"], serde_json::json!(["./src/*"]));

        // 2. has_named_export_symbol respects word boundaries; dual-export adds missing `export { Card }`
        assert!(!crate::sanitizer::has_named_export_symbol("export const CardHeader = () => null;", "Card"));
        assert!(crate::sanitizer::has_named_export_symbol("export const CardHeader = () => null;", "CardHeader"));
        let card_src = "export const CardHeader = () => null;\nconst Card = () => null;\nexport default Card;";
        let card_post = post_process_generated_file("src/components/Card.tsx", card_src, &serde_json::json!({}), false);
        assert!(card_post.contains("export { Card };"));

        // 3. deduplicate_repeated_css_blocks collapses repeated CSS rule loops
        let mut looped_css = String::from("body { margin: 0; }\n");
        for _ in 0..8 {
            looped_css.push_str(".table-responsive { display: block; width: 100%; overflow-x: auto; }\n");
        }
        let deduped_css = crate::sanitizer::deduplicate_repeated_css_blocks(&looped_css);
        assert_eq!(deduped_css.matches(".table-responsive").count(), 1);
        assert_eq!(deduped_css.matches("body { margin: 0; }").count(), 1);

        // 4. ensure_next_server_imports fixes default NextResponse import and injects missing imports
        let bad_mw = "import NextResponse from 'next/server';\nexport function middleware(req: NextRequest) { return NextResponse.next(); }\n";
        let fixed_mw = crate::sanitizer::ensure_next_server_imports(bad_mw, "middleware.ts");
        assert!(fixed_mw.contains("NextResponse"));
        assert!(fixed_mw.contains("NextRequest"));
        assert!(!fixed_mw.contains("import NextResponse from"));
        assert!(fixed_mw.contains("{ NextResponse") && fixed_mw.contains("NextRequest }"));

        // 5. Client hook preserves `import type` from @prisma/client while stripping value @prisma/client & ../lib/prisma
        let hook_src = r#"import { useState } from 'react';
import { prisma } from '../lib/prisma';
import { PrismaClient } from '@prisma/client';
import type { MaintenanceAlert } from '@prisma/client';

export function useMaintenanceAlerts() {
  const [alerts, setAlerts] = useState<MaintenanceAlert[]>([]);
  return { alerts, setAlerts };
}
"#;
        let processed_hook = post_process_generated_file("src/hooks/useMaintenanceAlerts.ts", hook_src, &serde_json::json!({}), false);
        assert!(processed_hook.trim_start().starts_with("\"use client\";"));
        assert!(!processed_hook.contains("../lib/prisma"));
        assert!(!processed_hook.contains("@/lib/prisma"));
        assert!(!processed_hook.contains("PrismaClient"));
        assert!(processed_hook.contains("import type { MaintenanceAlert } from '@prisma/client';"));

        // 6. sanitize_root_layout_direct_use_session neutralizes direct calls & prunes unused useSession import
        let root_with_session = "import { useSession, signOut } from 'next-auth/react';\nexport default function RootLayout({ children }: any) {\n  const { data: sess, status } = useSession();\n  return <html><body data-status={status}>{children}</body></html>;\n}\n";
        let neutralized = crate::sanitizer::sanitize_root_layout_direct_use_session(root_with_session);
        assert!(!neutralized.contains("useSession("));
        assert!(neutralized.contains("const sess: any = null;"));
        assert!(neutralized.contains("const status = 'unauthenticated';"));
        assert!(!neutralized.contains("useSession,"));
        assert!(neutralized.contains("{ signOut }"));

        // 7. ensure_ui_primitives_reconciled relaxes strict unions & appends missing Card / MetricCard / Option
        std::fs::write(
            temp_dir.join("src/components/ui.tsx"),
            "import React from 'react';\nexport interface ButtonProps {\n  variant?: 'primary' | 'secondary';\n  size?: 'sm' | 'md';\n}\nexport const CardHeader = ({ children }: any) => <div>{children}</div>;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/dashboard/page.tsx"),
            "import { Card, CardHeader, MetricCard, Option } from '@/components/ui';\nexport default function Dashboard() { return <Card><CardHeader>Hi</CardHeader><MetricCard value={1} /><select><Option value='1'>One</Option></select></Card>; }\n",
        )
        .unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let ui_after = std::fs::read_to_string(temp_dir.join("src/components/ui.tsx")).unwrap();
        assert!(ui_after.contains("| (string & {})"));
        assert!(ui_after.contains("export const Card ="));
        assert!(ui_after.contains("export const MetricCard ="));
        assert!(ui_after.contains("export const Option ="));
        assert_eq!(ui_after.matches("export const CardHeader").count(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_prisma_graph_sanitization_and_lbg_contracts_v48() {
        let broken_prisma = r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

enum Status {
  ACTIVE
  ARCHIVED
}

model Category {
  id       String     @id @default(cuid())
  parentId String?
  parent   Category?  @relation("CategoryHierarchy", fields: [parentId], references: [id])
  children Category[] @relation("CategoryHierarchy")
}

model Record {
  id      String   @id @default(cuid())
  status  Status   @default(ACTIVE)
  records Record[]
  events  Event[]
  tags    Tag[]
}

model Event {
  id    String @id @default(cuid())
  title String
}

model Tag {
  id      String   @id @default(cuid())
  name    String
  records Record[]
}
"#;
        let sanitized_prisma = sanitize_prisma_relations(broken_prisma);
        assert!(sanitized_prisma.contains("children Category[] @relation(\"CategoryHierarchy\")"));
        let record_block = sanitized_prisma
            .split("model Record")
            .nth(1)
            .and_then(|s| s.split("model ").next())
            .unwrap_or("");
        assert!(
            !record_block.contains("records Record[]"),
            "Expected self-list pruned from model Record:\n{}",
            record_block
        );
        assert!(
            !record_block.contains("Event[]"),
            "Expected unilateral Event[] pruned from model Record:\n{}",
            record_block
        );
        assert!(record_block.contains("tags    Tag[]") || record_block.contains("tags Tag[]"));
        // Verify Tag still keeps bilateral `records Record[]` while Record drops self-list `records Record[]`
        let tag_block = sanitized_prisma.split("model Tag").nth(1).unwrap_or("");
        assert!(tag_block.contains("records Record[]"));

        let schema_with_missing_type = r#"model Member {
  id     String @id @default(cuid())
  ghosts GhostEntity[]
  meta   Unsupported("tsvector")?
}
"#;
        let pruned_unknown = crate::sanitizer::prune_unresolved_prisma_relations(schema_with_missing_type);
        assert!(!pruned_unknown.contains("GhostEntity"));
        assert!(pruned_unknown.contains("Unsupported(\"tsvector\")"));

        let lbg = serde_json::json!({
            "sharedContracts": {
                "database": {
                    "models": {
                        "Member": {
                            "fields": ["id", "name"],
                            "relations": [
                                "records: Record[]",
                                "members: Member[]",
                                "missing: UnknownModel[]"
                            ]
                        },
                        "Event": {
                            "fields": ["id", "memberId"],
                            "relations": [
                                "member: Member",
                                "auditLogs: AuditEntry[]"
                            ]
                        },
                        "AuditEntry": {
                            "fields": ["id", "action"],
                            "relations": []
                        },
                        "Category": {
                            "fields": ["id", "parentId"],
                            "relations": [
                                "parent: Category?",
                                "children: Category[]"
                            ]
                        }
                    }
                }
            }
        });
        let clean_lbg = crate::orchestrator::sanitize_live_builder_guide_contracts(&lbg);
        let member_rels = clean_lbg["sharedContracts"]["database"]["models"]["Member"]["relations"].as_array().unwrap();
        assert!(member_rels.is_empty(), "Expected orphan/unknown relations pruned: {:?}", member_rels);
        let event_rels = clean_lbg["sharedContracts"]["database"]["models"]["Event"]["relations"].as_array().unwrap();
        assert_eq!(event_rels.len(), 1);
        assert_eq!(event_rels[0].as_str(), Some("member: Member"));
        let cat_rels = clean_lbg["sharedContracts"]["database"]["models"]["Category"]["relations"].as_array().unwrap();
        assert_eq!(cat_rels.len(), 2);

        // Single-model partial contract must be preserved unchanged
        let single_lbg = serde_json::json!({
            "sharedContracts": {
                "database": {
                    "models": {
                        "Module": {
                            "fields": ["id", "title"],
                            "relations": ["items: Item[]"]
                        }
                    }
                }
            }
        });
        let single_after = crate::orchestrator::sanitize_live_builder_guide_contracts(&single_lbg);
        assert_eq!(single_after["sharedContracts"]["database"]["models"]["Module"]["relations"].as_array().unwrap().len(), 1);
    }


    #[test]
    fn test_prisma_sanitizer_datasource_enum_defaults_and_scoring() {
        let broken_schema = "enum ItemState {\n  ACTIVE\n  ARCHIVED\n}\n\n\
model WorkItem {\n\
  id    String    @id @default(cuid())\n\
  state ItemState @default(ItemState.QUEUED)\n\
}\n\n\
model Account {\n\
  id                String  @id @default(cuid())\n\
  userId            String\n\
  type              String\n\
  provider          String\n\
  providerAccountId String\n\
  user              User    @relation(fields: [userId], references: [id], onDelete: Cascade)\n\
}\n";
        let processed = post_process_generated_file(
            "prisma/schema.prisma",
            broken_schema,
            &serde_json::json!({}),
            false,
        );
        assert!(processed.contains("generator client"), "Expected generator client:\n{}", processed);
        assert!(processed.contains("datasource db"), "Expected datasource db:\n{}", processed);
        assert!(processed.contains("env(\"DATABASE_URL\")"), "Expected DATABASE_URL env:\n{}", processed);
        assert!(processed.contains("@default(QUEUED)"), "Expected stripped enum qualifier:\n{}", processed);
        assert!(!processed.contains("@default(ItemState.QUEUED)"));
        assert!(processed.contains("QUEUED"), "Expected QUEUED added to enum ItemState:\n{}", processed);
        assert!(processed.contains("model User {"), "Expected missing NextAuth User model:\n{}", processed);

        let missing_ds = "Error: You don't have any datasource defined in your schema.prisma.";
        let cli_abort = "Error: P1012 schema parse error (unexpected token)";
        let dmmf_errs = "Error code: P1012\nerror: Field broken\nerror: Field two\nerror: Field three\nValidation Error Count: 3";
        let s_ds = crate::healer::score_prisma_error_log(missing_ds);
        let s_abort = crate::healer::score_prisma_error_log(cli_abort);
        let s_dmmf = crate::healer::score_prisma_error_log(dmmf_errs);
        assert!(s_ds > s_abort && s_abort > s_dmmf && s_dmmf == 3, "scores: ds={}, abort={}, dmmf={}", s_ds, s_abort, s_dmmf);
    }


    #[test]
    fn test_prisma_contract_summary_for_healing_v51() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_prisma_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            r#"
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

enum TicketState {
  OPEN
  CLOSED
}

model Ticket {
  id        String      @id @default(cuid())
  title     String
  state     TicketState @default(OPEN)
  createdAt DateTime    @default(now())
}
"#,
        )
        .unwrap();

        let summary = crate::self_healing::extract_prisma_schema_summary_for_healing(&temp_dir);
        assert!(summary.contains("Ticket (prisma.ticket)"), "Expected model/delegate in summary:\n{}", summary);
        assert!(summary.contains("TicketState: OPEN | CLOSED"), "Expected enum variants in summary:\n{}", summary);
        assert!(summary.contains("required: [title]"), "Expected required scalar field in summary:\n{}", summary);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_nextlink_nextauth_and_prisma_enum_reconciliation_v51() {
        // 1. Named { Link } -> default Link
        let bad_link = "import React from 'react';\nimport { Link } from 'next/link';\nexport default function Nav() { return <Link href=\"/\">Home</Link>; }\n";
        let fixed_link = crate::sanitizer::sanitize_next_link_named_import(bad_link);
        assert!(fixed_link.contains("import Link from 'next/link';"), "Expected default Link import:\n{}", fixed_link);
        assert!(!fixed_link.contains("{ Link }"));

        // 2. NextAuth provider named import & missing default NextAuth import
        let bad_auth = "import { NextAuthOptions, CredentialsProvider } from \"next-auth\";\nexport const authOptions: NextAuthOptions = { providers: [CredentialsProvider({ name: 'cred', credentials: {}, async authorize() { return null; } })] };\nexport default NextAuth(authOptions);\n";
        let fixed_auth = crate::sanitizer::sanitize_nextauth_imports(bad_auth);
        assert!(fixed_auth.contains("import CredentialsProvider from 'next-auth/providers/credentials';"), "Expected provider subpath import:\n{}", fixed_auth);
        assert!(fixed_auth.contains("import NextAuth from 'next-auth';"), "Expected default NextAuth import:\n{}", fixed_auth);
        assert!(fixed_auth.contains("import { NextAuthOptions } from \"next-auth\";"), "Expected NextAuthOptions preserved:\n{}", fixed_auth);

        // 3. Server Prisma.<Enum> stripping & @prisma/client import injection + project-context validator coverage
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_prisma_enum_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            "datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") }\nenum PaymentMethod {\n  STRIPE\n  CREDIT_CARD\n}\nmodel Invoice {\n  id     String        @id @default(cuid())\n  method PaymentMethod @default(STRIPE)\n}\n",
        ).unwrap();

        let bad_prisma_util = "import type { PrismaClient } from '@prisma/client';\nexport function isCard(m: Prisma.PaymentMethod) {\n  return m === Prisma.PaymentMethod.CREDIT_CARD;\n}\n";
        let fixed_prisma_util = crate::sanitizer::reconcile_server_prisma_enum_usage(&temp_dir, "src/utils/prisma.ts", bad_prisma_util);
        assert!(!fixed_prisma_util.contains("Prisma.PaymentMethod"), "Expected Prisma.PaymentMethod rewritten to top-level enum:\n{}", fixed_prisma_util);
        assert!(fixed_prisma_util.contains("import { PaymentMethod } from '@prisma/client';"), "Expected runtime enum import injected:\n{}", fixed_prisma_util);
        assert!(fixed_prisma_util.contains("PaymentMethod.CREDIT_CARD"));

        // 4. Abstract & project-context validators reject foreign frameworks, { Link }, { CredentialsProvider }, and non-API bad Prisma delegate
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/utils/i18n.ts",
            "import { createI18n } from 'vue-i18n';\nexport const i18n = createI18n({});\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/hardware/page.tsx",
            bad_link,
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/utils/auth.ts",
            bad_auth,
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(
            &temp_dir,
            "src/utils/prisma.ts",
            "import prisma from '@/lib/prisma';\nexport function load() { return prisma.ghostModel.findMany(); }\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(
            &temp_dir,
            "src/utils/prisma.ts",
            &format!("import prisma from '@/lib/prisma';\n{}\n", fixed_prisma_util),
            "qwen2.5-coder:7b"
        )).is_ok());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


