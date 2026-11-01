using UnrealBuildTool;

public class ReBoneBaseEditor : ModuleRules
{
	public ReBoneBaseEditor(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new[] { "Core", "CoreUObject", "Engine", "Paper2D", "ReBoneBase" });
		PrivateDependencyModuleNames.AddRange(new[] { "UnrealEd", "AssetRegistry" });
	}
}
