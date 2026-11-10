local dirpages = {'anthologies', 'doujins', 'issues', 'series'}

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'f5bc5d44e9f24a7a9afb40788acf20e3'
	m.Category                 = 'English-Scanlation'
	m.Name                     = 'DynastyScans'
	m.RootURL                  = 'https://dynasty-scans.com'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.TotalDirectory           = #dirpages
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/' .. dirpages[MODULE.CurrentDirectoryIndex + 1] .. '?page=' .. (URL + 1)) then
		local x = CreateTXQuery(HTTP.Document)
		x.XPathHREFAll('//dd/a', LINKS, NAMES)
		UPDATELIST.CurrentDirectoryPageNumber = tonumber(x.XPathString('//div[contains(@class, "pagination")]//li[last()-1]/a')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Title      = x.XPathString('//h2[@class="tag-title"]/b')
		if MANGAINFO.Title == '' then MANGAINFO.Title = x.XPathString('//h3[@id="chapter-title"]/b') end
		if MANGAINFO.Title == '' then MANGAINFO.Title = x.XPathString('//h2/b/substring-after(., "›")') end
		MANGAINFO.CoverLink  = MaybeFillHost(MODULE.RootURL,x.XPathString('//img[@class="thumbnail"]/@src'))
		MANGAINFO.Authors    = x.XPathString('string-join(//a[contains(@href, "/authors/")],", ")')
		MANGAINFO.Genres     = x.XPathStringAll('//*[@class="label" or @class="doujin_tags"]')
		MANGAINFO.Status     = MangaInfoStatusIfPos(x.XPathString('//h2[@class="tag-title"]/small'))
		MANGAINFO.Summary    = x.XPathString('//*[@class="description"]')
		local p = 1
		local pages = tonumber(x.XPathString('//div[contains(@class, "pagination")]//li[last()-1]/a'))
		if pages == nil then pages = 1 end
		while true do
			x.XPathHREFAll('//dl[@class="chapter-list"]/dd/a[1]', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
			p = p + 1
			if p > pages then
				break
			elseif HTTP.GET(MaybeFillHost(MODULE.RootURL, URL .. '?page=' .. tostring(p))) then
				x.ParseHTML(HTTP.Document)
			else
				break
			end
		end
		if MANGAINFO.ChapterLinks.Count == 0 then
			MANGAINFO.ChapterLinks.Add(URL)
			MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		end
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL,URL)) then
		CreateTXQuery(HTTP.Document).XPathStringAll('json(//script[contains(., "var pages")]/substring-after(substring-before(., ";"), " = "))()/concat("' .. MODULE.RootURL .. '", ./image)', TASK.PageLinks)
		return true
	else
		return false
	end
end
