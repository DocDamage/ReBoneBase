using UnrealBuildTool;

public class ReBoneBase : ModuleRules
{
	public ReBoneBase(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new[] { "Core", "CoreUObject", "Engine", "Paper2D" });
	}
}
